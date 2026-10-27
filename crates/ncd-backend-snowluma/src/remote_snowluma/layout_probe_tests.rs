// 远端安装探测使用发行文件识别首次启动前的 SnowLuma。

use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality, Os,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const HOME: &str = "/home/u";
const PACKAGE: &str = r#"{"name":"@snowluma/runtime","version":"2.0.0","main":"index.mjs"}"#;

#[derive(Default)]
struct FileHost {
    files: BTreeMap<String, Bytes>,
    executables: BTreeSet<String>,
    path_node: Option<String>,
}

impl FileHost {
    fn file(mut self, path: impl Into<String>, contents: &str) -> Self {
        self.files
            .insert(path.into(), Bytes::copy_from_slice(contents.as_bytes()));
        self
    }

    fn executable(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.files.insert(path.clone(), Bytes::new());
        self.executables.insert(path);
        self
    }

    fn release(self, dir: &str) -> Self {
        self.file(format!("{dir}/index.mjs"), "// SnowLuma bundle")
            .file(format!("{dir}/package.json"), PACKAGE)
    }

    fn with_path_node(mut self, path: &str) -> Self {
        self.path_node = Some(path.to_string());
        self
    }
}

fn unsupported<T>() -> Result<T, HostError> {
    Err(HostError::Unsupported {
        operation: "layout probe fixture",
    })
}

#[async_trait]
impl Host for FileHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "layout-probe"
    }
    fn shell(&self) -> &dyn HostShell {
        &ncd_host::shell::BashShell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.files
            .get(path.as_posix())
            .cloned()
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        Ok(self.files.contains_key(path.as_posix()))
    }
    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        assert_eq!(cmd.program, "sh");
        let script = cmd.args.last().expect("shell probe script");
        let stdout = match script.as_str() {
            "echo \"$HOME\"" => Some(format!("{HOME}\n")),
            "command -v node" => self.path_node.as_ref().map(|path| format!("{path}\n")),
            _ if script.starts_with("test -f ") && script.contains(" && test -x ") => {
                self.executables.iter().find_map(|path| {
                    let quoted = shell_single_quote(path);
                    (script == &format!("test -f {quoted} && test -x {quoted}")).then(String::new)
                })
            }
            _ => panic!("unexpected layout probe: {script}"),
        };
        Ok(CommandOutput {
            exit_code: Some(if stdout.is_some() { 0 } else { 1 }),
            stdout: stdout.unwrap_or_default(),
            stderr: String::new(),
        })
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unsupported()
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unsupported()
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported()
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported()
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported()
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        unsupported()
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        unsupported()
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        unsupported()
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unsupported()
    }
}

fn selected_home() -> RemoteSelectedPaths {
    RemoteSelectedPaths {
        home: HOME.into(),
        ..RemoteSelectedPaths::default()
    }
}

#[tokio::test]
async fn fresh_full_release_without_config_uses_its_bundled_node() {
    let paths = SnowLumaRemotePaths::from_remote_home(HOME);
    let host = FileHost::default()
        .release(&paths.snowluma_dir)
        .executable(paths.bundled_node_bin())
        .executable(paths.node_bin())
        .executable(napcat_layout_qq_executable(HOME))
        .with_path_node("/usr/bin/node");

    let layout = probe_remote_snowluma_layout(&host).await.unwrap();

    assert_eq!(layout.paths, paths);
    assert_eq!(layout.node_bin, paths.bundled_node_bin());
    assert_eq!(layout.qq_bin, napcat_layout_qq_executable(HOME));
    assert!(host.files.keys().all(|path| !path.contains("/config/")));
}

#[tokio::test]
async fn fresh_lite_release_without_config_uses_workspace_node_before_path() {
    let paths = SnowLumaRemotePaths::from_remote_home(HOME);
    let host = FileHost::default()
        .release(&paths.snowluma_dir)
        .executable(paths.node_bin())
        .executable(napcat_layout_qq_executable(HOME))
        .with_path_node("/usr/bin/node");

    let layout = layout_from_selected_or_probe(&host, &selected_home())
        .await
        .unwrap();

    assert_eq!(layout.node_bin, paths.node_bin());
}

#[tokio::test]
async fn fresh_lite_system_install_without_config_uses_path_node() {
    let dir = "/opt/snowluma";
    let host = FileHost::default()
        .release(dir)
        .executable("/opt/QQ/qq")
        .with_path_node("/usr/local/bin/node");

    let layout = layout_from_selected_or_probe(&host, &selected_home())
        .await
        .unwrap();

    assert_eq!(layout.paths.snowluma_dir, dir);
    assert_eq!(layout.node_bin, "/usr/local/bin/node");
    assert_eq!(layout.qq_install_base, "/");
}

#[tokio::test]
async fn install_identity_still_requires_the_entry_file() {
    let dir = "/opt/snowluma";
    let host = FileHost::default()
        .file(format!("{dir}/package.json"), PACKAGE)
        .file(format!("{dir}/config/webui.json"), "{}")
        .file(
            format!("{dir}/config/runtime.json"),
            r#"{"webuiPort":5099}"#,
        );

    assert!(!snowluma_dir_is_install(&host, dir).await);
}

#[tokio::test]
async fn unrelated_missing_or_corrupt_manifests_do_not_identify_a_fresh_install() {
    let dir = "/opt/snowluma";
    for manifest in [
        None,
        Some(r#"{"name":"another-app","version":"2.0.0"}"#),
        Some(r#"{"version":"2.0.0"}"#),
        Some("{invalid json"),
    ] {
        let mut host = FileHost::default().file(format!("{dir}/index.mjs"), "export {};");
        if let Some(manifest) = manifest {
            host = host.file(format!("{dir}/package.json"), manifest);
        }
        assert!(
            !snowluma_dir_is_install(&host, dir).await,
            "accepted unrelated installation: {manifest:?}"
        );
    }
}

#[tokio::test]
async fn legacy_webui_config_remains_an_install_marker_without_manifest() {
    let dir = "/opt/snowluma";
    let host = FileHost::default()
        .file(format!("{dir}/index.mjs"), "// legacy bundle")
        .file(format!("{dir}/config/webui.json"), "{}");

    assert!(snowluma_dir_is_install(&host, dir).await);
}

#[tokio::test]
async fn legacy_runtime_port_remains_an_install_marker_without_manifest() {
    let dir = "/opt/snowluma";
    let host = FileHost::default()
        .file(format!("{dir}/index.mjs"), "// legacy bundle")
        .file(
            format!("{dir}/config/runtime.json"),
            r#"{"webuiPort":5099}"#,
        );

    assert!(snowluma_dir_is_install(&host, dir).await);
}

#[tokio::test]
async fn discovery_skips_an_invalid_earlier_candidate() {
    let candidates = snowluma_install_candidates(HOME);
    let host = FileHost::default()
        .file(format!("{}/index.mjs", candidates[0]), "export {};")
        .file(
            format!("{}/package.json", candidates[0]),
            r#"{"name":"another-app"}"#,
        )
        .release(&candidates[1]);

    assert_eq!(
        find_snowluma_install(&host, HOME).await.as_deref(),
        Some(candidates[1].as_str())
    );
}

#[tokio::test]
async fn selected_custom_install_and_portable_node_are_not_replaced() {
    let selected = RemoteSelectedPaths {
        home: HOME.into(),
        snowluma_dir: Some("/srv/custom-sl".into()),
        snowluma_workspace: Some("/srv/custom-workspace".into()),
        node_bin: Some("/srv/custom-workspace/node/bin/node".into()),
        qq_install_base: Some("/srv/qq".into()),
        qq_bin: Some("/srv/qq/opt/QQ/qq".into()),
        ..RemoteSelectedPaths::default()
    };
    let host = FileHost::default()
        .release("/srv/custom-sl")
        .release(&snowluma_install_candidates(HOME)[0])
        .with_path_node("/usr/bin/node");

    let layout = layout_from_selected_or_probe(&host, &selected)
        .await
        .unwrap();

    assert_eq!(layout.paths.snowluma_dir, "/srv/custom-sl");
    assert_eq!(layout.paths.workspace_dir, "/srv/custom-workspace");
    assert_eq!(layout.node_bin, "/srv/custom-workspace/node/bin/node");
    assert_eq!(layout.qq_bin, "/srv/qq/opt/QQ/qq");
}
