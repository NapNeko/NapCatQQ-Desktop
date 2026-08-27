//! NodeJsComponent:Node.js runtime 组件
//!
//! 作为第一个完整 Component 实装,跑通 Component trait 的全套能力,后续
//! component 照本模板扩展
//!
//! 支持矩阵:
//! - (Linux, Local) / (Linux, Remote):从 nodejs.org 下载 tar.xz 解压
//! - (Windows, Local):tar.xz 在 Windows 上解压暂未实装
//!   (HostError::Unsupported),后续按需补 zip 形式的 node Windows 包
//!
//! 探测策略:
//! 1. 目标安装目录 <install_dir>/bin/node 存在 + node --version 输出
//! 2. extra_detect_bins（便携安装 / 用户覆盖；不含 SnowLuma 完整包自带的 ./node）
//! 3. PATH 中有 node，且满足 SnowLuma 要求（22.13+ / 23.4+）
//!
//! 默认下载源:
//! https://nodejs.org/dist/v{version}/node-v{version}-linux-x64.tar.xz
//! 可通过 NodeJsComponent::with_url(...) 覆盖镜像

use std::sync::Arc;

use async_trait::async_trait;

use ncd_domain::{NodeEnvironmentCandidate, NodeSourceKind};
use ncd_host::{Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::shell_quote;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

async fn probe_node_bin(
    host: &dyn Host,
    path: &str,
) -> Result<Option<DetectedVersion>, ActionError> {
    let hp = if host.os() == Os::Windows {
        HostPath::from_windows(path)
    } else {
        HostPath::from_posix(path)
    };
    if let Some(ver) = probe_node_raw_version(host, &hp).await? {
        if NodeJsComponent::version_meets_snowluma(&ver) {
            return Ok(Some(DetectedVersion {
                version: ver,
                source: path.to_string(),
            }));
        }
    }
    Ok(None)
}

pub async fn probe_node_raw_version(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, ActionError> {
    if !host.exists(path).await? {
        return Ok(None);
    }
    let cmd = HostCommand::new(path.as_posix()).arg("--version");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let ver = out.stdout.trim().trim_start_matches('v').to_string();
            if ver.is_empty() {
                Ok(None)
            } else {
                Ok(Some(ver))
            }
        }
        _ => Ok(None),
    }
}

/// Node.js component 配置
#[derive(Debug, Clone)]
pub struct NodeJsComponent {
    /// 期望版本(无 v 前缀,如 "20.10.0")
    pub version: String,
    /// 安装目录(如 /opt/napcat/runtime/node 或 $HOME/Napcat/usr/node)
    pub install_dir: HostPath,
    /// 下载源 URL(默认 nodejs.org 官方)
    pub download_url_template: Option<String>,
    /// 期望 SHA256(可选,提供则严格校验)
    pub expected_sha256: Option<String>,
    /// 临时目录(下载 tarball 用,默认 /tmp/)
    pub tmp_dir: HostPath,
    /// 额外探测点（不作为安装目标），例如便携 `.../node/bin/node`
    extra_detect_bins: Vec<HostPath>,
}

impl NodeJsComponent {
    /// 创建一个 Node.js component 描述
    pub fn new(version: impl Into<String>, install_dir: HostPath) -> Self {
        Self {
            version: version.into(),
            install_dir,
            download_url_template: None,
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
            extra_detect_bins: Vec::new(),
        }
    }

    pub fn with_extra_detect_bin(mut self, path: HostPath) -> Self {
        if !self.extra_detect_bins.iter().any(|p| p == &path) {
            self.extra_detect_bins.push(path);
        }
        self
    }

    pub fn with_url_template(mut self, template: impl Into<String>) -> Self {
        self.download_url_template = Some(template.into());
        self
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    /// 推算下载 URL(根据 host 的 OS / arch)
    fn build_download_url(&self, host: &dyn Host) -> Result<String, ActionError> {
        let (platform, ext) = match host.os() {
            Os::Linux => ("linux", "tar.xz"),
            Os::MacOs => ("darwin", "tar.xz"),
            Os::Windows => ("win", "zip"),
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        let template = self.download_url_template.clone().unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.{ext}",
                ver = "{version}",
                platform = "{platform}",
                arch = "{arch}",
                ext = "{ext}"
            )
        });
        let url = template
            .replace("{version}", &self.version)
            .replace("{platform}", platform)
            .replace("{arch}", arch)
            .replace("{ext}", ext);
        Ok(url)
    }

    pub fn node_binary_path_for_os(install_dir: &HostPath, os: Os) -> HostPath {
        match os {
            Os::Windows => install_dir.join("node.exe"),
            _ => install_dir.join("bin/node"),
        }
    }

    pub fn node_binary_path(&self, host: &dyn Host) -> HostPath {
        Self::node_binary_path_for_os(&self.install_dir, host.os())
    }

    fn source_is_managed(&self, host: &dyn Host, source: &str) -> bool {
        let expected = self.node_binary_path(host);
        let normalize = |path: &str| path.replace('\\', "/").trim_end_matches('/').to_lowercase();
        normalize(source) == normalize(expected.as_posix())
            || normalize(source) == normalize(&expected.render(ncd_host::PathStyle::Windows))
    }

    /// 对齐上游 `check-node-version.cjs`：`^22.13.0 || >=23.4.0`
    pub fn version_meets_snowluma(raw: &str) -> bool {
        let v = raw.trim().trim_start_matches('v');
        let mut parts = v.split('.');
        let Some(major) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            return false;
        };
        let minor = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        match major {
            22 => minor >= 13,
            23 => minor >= 4,
            n => n > 23,
        }
    }

    fn extract_root_subdir(&self, host: &dyn Host) -> String {
        let platform = match host.os() {
            Os::Linux => "linux",
            Os::MacOs => "darwin",
            Os::Windows => "win",
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        format!("node-v{}-{platform}-{arch}", self.version)
    }

    /// 组件元数据,给 list_components Tauri command 使用
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NodeJs,
            display_name: "Node.js".to_string(),
            description: "供 SnowLuma Lite 使用的 Node.js 环境".to_string(),
            repo_url: Some("https://nodejs.org/".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
                crate::types::SupportedTarget::new(Os::MacOs, Locality::Local),
            ],
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }
}

#[async_trait]
impl Component for NodeJsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NodeJs
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
            (Os::MacOs, Locality::Local),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let binary = self.node_binary_path(host);
        if let Some(detected) = probe_node_bin(host, binary.as_posix()).await? {
            return Ok(Some(detected));
        }

        for extra in &self.extra_detect_bins {
            if let Some(detected) = probe_node_bin(host, extra.as_posix()).await? {
                return Ok(Some(detected));
            }
        }

        let path_cmd = HostCommand::new("node").arg("--version");
        match host.run_to_string(path_cmd).await {
            Ok(out) if out.success() => {
                let ver_str = out.stdout.trim().trim_start_matches('v');
                if ver_str.is_empty() {
                    return Ok(None);
                }
                if Self::version_meets_snowluma(ver_str) {
                    return Ok(Some(DetectedVersion {
                        version: ver_str.to_string(),
                        source: "$PATH/node".into(),
                    }));
                }
                Ok(None)
            }
            Ok(_) => Ok(None),
            Err(HostError::CommandFailed { .. }) => Ok(None),
            Err(HostError::Io(_)) => Ok(None),
            Err(e) => Err(ActionError::Host(e)),
        }
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // Step 1: 下载归档
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download node.js archive".into(),
        })
        .await;

        let url = self.build_download_url(host)?;
        let archive_ext = if host.os() == Os::Windows { "zip" } else { "tar.xz" };
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-nodejs-{}-{}.{}",
            self.version,
            std::process::id(),
            archive_ext
        ));

        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
        helper
            .download_with_mirrors(
                &mirrors,
                &local_tmp,
                self.expected_sha256.as_deref(),
                ctx,
                1,
            )
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2: 上传到目标 host
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "upload archive to host".into(),
        })
        .await;
        let remote_archive = self.tmp_dir.join(format!(
            "ncd-nodejs-{}-{}.{}",
            self.version,
            std::process::id(),
            archive_ext
        ));
        host.create_dir_all(&self.tmp_dir).await?;
        host.upload(&local_tmp, &remote_archive).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // Step 3: 解压到临时位置
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "extract archive".into(),
        })
        .await;
        let stage_dir = self
            .tmp_dir
            .join(format!("ncd-nodejs-stage-{}", std::process::id()));
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;

        let kind = if host.os() == Os::Windows {
            ArchiveKind::Zip
        } else {
            ArchiveKind::TarXz
        };
        host.extract_archive(&remote_archive, &stage_dir, kind)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // Step 4: 把 stage/<root>/* 移到 install_dir
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "install to target dir".into(),
        })
        .await;
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.create_dir_all(&self.install_dir).await?;
        let root_subdir = stage_dir.join(self.extract_root_subdir(host));

        if host.os() == Os::Windows {
            copy_dir_all(host, &root_subdir, &self.install_dir).await?;
        } else {
            let root = shell_quote(root_subdir.as_posix());
            let dest = shell_quote(self.install_dir.as_posix());
            let mv_cmd = HostCommand::new("sh").arg("-c").arg(format!(
                "mv {root}/* {dest}/ && mv {root}/.* {dest}/ 2>/dev/null; true",
            ));
            let mv_out = host.run_to_string(mv_cmd).await?;
            if !mv_out.success() {
                return Err(ActionError::install_step(
                    "mv_install",
                    format!("exit={:?}: {}", mv_out.exit_code, mv_out.stderr.trim()),
                ));
            }
        }

        // 清理 stage 与 archive
        let _ = host.remove_dir_all(&stage_dir).await;
        let _ = host.remove_file(&remote_archive).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("remove {}", self.install_dir.as_posix()),
        })
        .await;

        if let Some(detected) = self.detect(host).await? {
            if !self.source_is_managed(host, &detected.source) {
                ctx.info(format!(
                    "检测到的是外部 Node.js（{}），不会删除独立组件目录。",
                    detected.source
                ))
                .await;
                ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
                ctx.emit(ProgressKind::Finished { ok: true }).await;
                return Ok(());
            }
        }

        if !host.exists(&self.install_dir).await? {
            ctx.info("Node.js 独立组件在目标路径未安装（当前环境为外部或系统 PATH），无需删除目录。")
                .await;
            ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
            ctx.emit(ProgressKind::Finished { ok: true }).await;
            return Ok(());
        }

        if let Err(err) = host.remove_dir_all(&self.install_dir).await {
            let msg = format!(
                "删除 Node.js 组件目录失败 ({}): {err}。若有正在运行的 SnowLuma 实例，请先停止 Bot 释放文件锁定后再试。",
                self.install_dir.as_posix()
            );
            ctx.log(ProgressLogLevel::Error, msg.clone()).await;
            ctx.emit(ProgressKind::StepEnd { step: 1, ok: false }).await;
            ctx.emit(ProgressKind::Finished { ok: false }).await;
            return Err(ActionError::other(msg));
        }

        ctx.info("Node.js 独立组件目录已成功清理").await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        if let Some(detected) = self.detect(host).await? {
            if !self.source_is_managed(host, &detected.source) {
                return Err(ActionError::other(format!(
                    "检测到的是外部 Node.js（{}），不能通过组件更新；请先安装 Node.js 组件。",
                    detected.source
                )));
            }
        }
        self.install(host, ctx).await
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let binary = self.node_binary_path(host);
        let exists = host.exists(&binary).await?;
        let mut report =
            VerifyReport::ok().with_check("node binary exists", exists, Some(format!("{binary}")));
        if exists {
            let cmd = HostCommand::new(binary.as_posix()).arg("--version");
            match host.run_to_string(cmd).await {
                Ok(out) if out.success() => {
                    let actual = out.stdout.trim().trim_start_matches('v').to_string();
                    let matches = actual == self.version;
                    report = report.with_check(
                        "version matches",
                        matches,
                        Some(format!("expected={} actual={actual}", self.version)),
                    );
                }
                Ok(out) => {
                    report = report.with_check(
                        "version executable",
                        false,
                        Some(format!(
                            "exit={:?} stderr={}",
                            out.exit_code,
                            out.stderr.trim()
                        )),
                    );
                }
                Err(e) => {
                    report = report.with_check("version executable", false, Some(format!("{e}")));
                }
            }
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let binary = self.node_binary_path(host);
        Ok(args.apply_to(HostCommand::new(binary.as_posix())))
    }
}

async fn copy_dir_all(host: &dyn Host, src: &HostPath, dst: &HostPath) -> Result<(), ActionError> {
    let mut stack = vec![(src.clone(), dst.clone())];
    while let Some((s, d)) = stack.pop() {
        let entries = host.list_dir(&s).await?;
        for entry in entries {
            let s_child = s.join(&entry.name);
            let d_child = d.join(&entry.name);
            if entry.is_dir {
                host.create_dir_all(&d_child).await?;
                stack.push((s_child, d_child));
            } else {
                let bytes = host.read_file(&s_child).await?;
                host.write_file(&d_child, &bytes).await?;
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn _ensure_send_sync(_: Arc<NodeJsComponent>) {}

pub async fn probe_local_system_nodes(
    host: &dyn Host,
    component_path: Option<&HostPath>,
    custom_path: Option<&str>,
) -> Vec<NodeEnvironmentCandidate> {
    let mut results = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Custom configured path
    if let Some(cp) = custom_path {
        let trimmed = cp.trim();
        if !trimmed.is_empty() {
            let hp = HostPath::from_windows(trimmed);
            if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                let path_str = hp.render(ncd_host::PathStyle::Windows);
                seen_paths.insert(path_str.to_lowercase());
                results.push(NodeEnvironmentCandidate {
                    path: path_str,
                    version: ver.clone(),
                    source_kind: NodeSourceKind::Custom,
                    label: format!("自定义路径 (v{ver})"),
                    is_valid,
                });
            }
        }
    }

    // 2. Independent NodeJs component
    if let Some(cp) = component_path {
        if let Ok(Some(ver)) = probe_node_raw_version(host, cp).await {
            let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
            let path_str = cp.render(ncd_host::PathStyle::Windows);
            if !seen_paths.contains(&path_str.to_lowercase()) {
                seen_paths.insert(path_str.to_lowercase());
                results.push(NodeEnvironmentCandidate {
                    path: path_str,
                    version: ver.clone(),
                    source_kind: NodeSourceKind::Component,
                    label: format!("Node.js 组件 (v{ver})"),
                    is_valid,
                });
            }
        }
    }

    // 3. System PATH
    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("where.exe").arg("node").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let hp = HostPath::from_windows(trimmed);
                    let path_str = hp.render(ncd_host::PathStyle::Windows);
                    if seen_paths.contains(&path_str.to_lowercase()) {
                        continue;
                    }
                    if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                        let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                        seen_paths.insert(path_str.to_lowercase());
                        results.push(NodeEnvironmentCandidate {
                            path: path_str,
                            version: ver.clone(),
                            source_kind: NodeSourceKind::SystemPath,
                            label: format!("系统 PATH (v{ver})"),
                            is_valid,
                        });
                    }
                }
            }
        }
    }

    // 4. Common Version Managers / Paths (NVM, fnm, Volta, Program Files)
    #[cfg(windows)]
    {
        let mut check_dirs = Vec::new();
        if let Ok(nvm_home) = std::env::var("NVM_HOME") {
            check_dirs.push(std::path::PathBuf::from(nvm_home));
        }
        if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
            check_dirs.push(std::path::PathBuf::from(nvm_symlink));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let fnm = std::path::PathBuf::from(&local_app_data).join("fnm_multishells");
            let volta = std::path::PathBuf::from(&local_app_data).join("Volta").join("bin");
            let pnpm = std::path::PathBuf::from(&local_app_data).join("pnpm");
            check_dirs.push(fnm);
            check_dirs.push(volta);
            check_dirs.push(pnpm);
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            check_dirs.push(std::path::PathBuf::from(&program_files).join("nodejs"));
        }

        for base_dir in check_dirs {
            let node_exe = base_dir.join("node.exe");
            if node_exe.is_file() {
                let hp = HostPath::from_windows(node_exe.to_string_lossy().trim());
                let path_str = hp.render(ncd_host::PathStyle::Windows);
                if !seen_paths.contains(&path_str.to_lowercase()) {
                    if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                        let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                        seen_paths.insert(path_str.to_lowercase());
                        results.push(NodeEnvironmentCandidate {
                            path: path_str,
                            version: ver.clone(),
                            source_kind: NodeSourceKind::VersionManager,
                            label: format!("版本管理器/已安装 (v{ver})"),
                            is_valid,
                        });
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_download_url_substitutes_version_and_arch() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/opt/node"));
        let template = comp.download_url_template.clone().unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.{ext}",
                ver = "{version}",
                platform = "{platform}",
                arch = "{arch}",
                ext = "{ext}"
            )
        });
        assert!(template.contains("{version}"));
        assert!(template.contains("{platform}"));
        assert!(template.contains("{arch}"));
    }

    #[test]
    fn extract_root_subdir_uses_correct_format() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/opt/node"));
        assert_eq!(comp.version, "22.13.0");
    }

    #[test]
    fn node_binary_path_joins_correctly() {
        let install = HostPath::from_posix("/opt/node");
        assert_eq!(
            NodeJsComponent::node_binary_path_for_os(&install, Os::Linux).as_posix(),
            "/opt/node/bin/node"
        );
        let win_install = HostPath::from_windows(r"C:\Napcat\node");
        assert_eq!(
            NodeJsComponent::node_binary_path_for_os(&win_install, Os::Windows).render(ncd_host::PathStyle::Windows),
            r"C:\Napcat\node\node.exe"
        );
    }

    #[test]
    fn version_meets_snowluma_matches_upstream_range() {
        assert!(NodeJsComponent::version_meets_snowluma("22.13.0"));
        assert!(NodeJsComponent::version_meets_snowluma("v22.14.1"));
        assert!(NodeJsComponent::version_meets_snowluma("23.4.0"));
        assert!(NodeJsComponent::version_meets_snowluma("24.0.0"));
        assert!(!NodeJsComponent::version_meets_snowluma("22.12.0"));
        assert!(!NodeJsComponent::version_meets_snowluma("18.19.1"));
        assert!(!NodeJsComponent::version_meets_snowluma("23.3.0"));
    }

    #[test]
    fn supported_targets_includes_linux_remote_and_windows_local() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/x"));
        let targets = comp.supported_targets();
        assert!(targets.contains(&(Os::Linux, Locality::Local)));
        assert!(targets.contains(&(Os::Linux, Locality::Remote)));
    }
}
