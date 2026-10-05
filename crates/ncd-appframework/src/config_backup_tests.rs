use super::*;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, HostCommand, HostError, HostProcess, HostShell, Locality,
};
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone)]
struct Node {
    bytes: Option<Vec<u8>>,
    symlink: bool,
}

struct TestHost {
    id: String,
    nodes: Mutex<BTreeMap<String, Node>>,
    fail_once: Mutex<Option<String>>,
    shell: ncd_host::shell::BashShell,
}

impl TestHost {
    fn new(id: &str) -> Arc<Self> {
        let host = Arc::new(Self {
            id: id.into(),
            nodes: Mutex::new(BTreeMap::new()),
            fail_once: Mutex::new(None),
            shell: ncd_host::shell::BashShell,
        });
        host.nodes.lock().unwrap().insert(
            String::new(),
            Node {
                bytes: None,
                symlink: false,
            },
        );
        host
    }

    fn mkdir(&self, path: &str) {
        let mut nodes = self.nodes.lock().unwrap();
        let mut cursor = String::new();
        for part in path.trim_matches('/').split('/') {
            cursor.push('/');
            cursor.push_str(part);
            nodes.entry(cursor.clone()).or_insert(Node {
                bytes: None,
                symlink: false,
            });
        }
    }

    fn put(&self, path: &str, bytes: &[u8]) {
        self.mkdir(path.rsplit_once('/').unwrap().0);
        self.nodes.lock().unwrap().insert(
            path.into(),
            Node {
                bytes: Some(bytes.to_vec()),
                symlink: false,
            },
        );
    }

    fn content(&self, path: &str) -> Option<Vec<u8>> {
        self.nodes
            .lock()
            .unwrap()
            .get(path)
            .and_then(|n| n.bytes.clone())
    }
}

#[async_trait::async_trait]
impl Host for TestHost {
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
        &self.id
    }
    fn shell(&self) -> &dyn HostShell {
        &self.shell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.content(path.as_posix())
            .map(Bytes::from)
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let fail = {
            let mut failure = self.fail_once.lock().unwrap();
            if failure.as_deref() == Some(path.as_posix()) {
                failure.take();
                true
            } else {
                false
            }
        };
        if fail {
            self.put(path.as_posix(), b"partial write");
            return Err(HostError::PermissionDenied {
                path: path.clone(),
                operation: "write",
            });
        }
        self.put(path.as_posix(), bytes);
        Ok(())
    }
    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        let cursor = path.as_posix().trim_end_matches('/');
        let nodes = self.nodes.lock().unwrap();
        if nodes.get(cursor).is_none_or(|n| n.bytes.is_some()) {
            return Err(HostError::PathNotFound { path: path.clone() });
        }
        let prefix = format!("{cursor}/");
        Ok(nodes
            .iter()
            .filter_map(|(p, n)| {
                let leaf = p.strip_prefix(&prefix)?;
                if leaf.is_empty() || leaf.contains('/') {
                    return None;
                }
                Some(DirEntry {
                    name: leaf.into(),
                    is_dir: n.bytes.is_none(),
                    size: n.bytes.as_ref().map_or(0, |b| b.len() as u64),
                    modified: None,
                    is_symlink: n.symlink,
                    mode: None,
                })
            })
            .collect())
    }
    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.mkdir(path.as_posix());
        Ok(())
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.nodes.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "remove_dir_all",
        })
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        Ok(self
            .nodes
            .lock()
            .unwrap()
            .contains_key(path.as_posix().trim_end_matches('/')))
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "upload",
        })
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "download",
        })
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "extract_archive",
        })
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(HostError::Unsupported { operation: "spawn" })
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        Err(HostError::Unsupported { operation: "run" })
    }
}

fn instance(framework: &str, id: &str) -> AppInstance {
    serde_json::from_value(
        serde_json::json!({"id":id,"framework_id":framework,"display_name":framework,
        "placement":"remote_native","host_id":"remote:example","install_dir":format!("/apps/{id}"),
        "port":7777,"state":"stopped","created_at_ms":1}),
    )
    .unwrap()
}

fn backup(framework: &str, id: &str, files: &[(&str, &str)]) -> FrameworkConfigBackup {
    FrameworkConfigBackup {
        version: 1,
        instance_id: id.into(),
        framework_id: framework.into(),
        files: files
            .iter()
            .map(|(path, text)| FrameworkConfigFile {
                relative_path: (*path).into(),
                text: (*text).into(),
            })
            .collect(),
    }
}

#[tokio::test]
async fn captures_all_builtin_framework_configs_without_programs_or_databases() {
    let registry = crate::AppFrameworkRegistry::with_builtin();
    let cases: &[(&str, &[(&str, &str)], &[&str])] = &[
        (
            "neobot",
            &[
                (
                    "data/config.toml",
                    "# keep\r\n[adapter]\nreverse_ws_port = 8080\n",
                ),
                ("plugins_data/dashboard/config.toml", "port = 9981\n"),
                ("plugins_data/my-plugin/config.toml", "enabled = true\n"),
            ],
            &[
                "plugins_data/dashboard/auth.json",
                "plugins_data/my-plugin/history.json",
                "data/chat.db",
                "plugins/my-plugin/main.py",
            ],
        ),
        (
            "karin",
            &[
                (".env", "# keep\r\nHTTP_PORT=7777\r\n"),
                ("@karinjs/config/adapter.json", "{\"custom\":42}"),
                (
                    "@karinjs/@karinjs-plugin-basic/config/nested/custom.yaml",
                    "# plugin\nvalue: yes\n",
                ),
                ("@karinjs/another/settings.json", "{}"),
            ],
            &[
                "node_modules/karin/config.json",
                "@karinjs/plugin/cache/messages.json",
            ],
        ),
        (
            "nonebot2",
            &[
                (".env", "ENVIRONMENT=dev\n"),
                (".env.dev", "# dev\nA=1"),
                (".env.prod", "A=2"),
                (
                    "pyproject.toml",
                    "[tool.nonebot]\nplugins = ['onebot_plugin']\n",
                ),
            ],
            &[".env.ncd.bak", "bot.py", ".venv/settings.json"],
        ),
        (
            "astrbot",
            &[
                (
                    "data/cmd_config.json",
                    "\u{feff}{\"provider\":[{\"unknown\":1}]}\n",
                ),
                (
                    "data/shared_preferences.json",
                    "{\"inactivated_plugins\":[]}",
                ),
                ("data/config/abconf_a.json", "{}"),
                ("data/config/test_config.json", "{\"enabled\":true}"),
            ],
            &[
                "data/data_v4.db",
                "data/plugins/test/main.py",
                "data/logs/cached.json",
            ],
        ),
        (
            "maibot",
            &[
                ("config/bot_config.toml", "# personality\nname = '麦麦'\n"),
                (
                    "config/model_config.toml",
                    "[[api_providers]]\nname = 'test'\n",
                ),
                ("data/webui.json", "{}"),
                ("plugins/MaiBot-Napcat-Adapter/config.toml", "port = 7777"),
                ("plugins/my-plugin/config.toml", "enabled = false"),
                ("data/custom_prompts/zh-CN/greet.prompt", "你好 {{name}}\n"),
                ("data/custom_prompts/zh-CN/.versions/greet/v1.prompt", "old"),
                (
                    "data/custom_prompts/zh-CN/.versions/greet/manifest.json",
                    "[]",
                ),
                ("data/custom_prompts/zh-CN/greet.meta.toml", "name = '问候'"),
            ],
            &[
                "plugins/my-plugin/main.py",
                "data/MaiBot.db",
                "prompts/zh-CN/greet.prompt",
            ],
        ),
        (
            "koishi",
            &[
                ("koishi.yml", "# preserve\nplugins:\n  group: {}\n"),
                (".env", "A=1"),
            ],
            &[
                // 可执行清单只导出 koishi.yml / .env，package.json 不随备份写回（yarn start 会跑 scripts.start）
                "package.json",
                "node_modules/plugin/config.json",
                "data/koishi.db",
            ],
        ),
        (
            "yunzai",
            &[
                ("config/config/bot.yaml", "# user\nmasterQQ: [10001]"),
                ("plugins/test/config/nested/settings.yaml", "value: 1"),
            ],
            &[
                "config/default_config/bot.yaml",
                "plugins/test/config/templates/default.yaml",
                "plugins/test/index.js",
                "data/db.json",
            ],
        ),
    ];
    let covered: HashSet<_> = cases
        .iter()
        .map(|(framework, _, _)| (*framework).to_string())
        .collect();
    let registered: HashSet<_> = registry
        .manifests()
        .iter()
        .map(|manifest| manifest.id.as_str().to_string())
        .collect();
    assert_eq!(covered, registered, "新增内置框架时也需要补齐配置备份覆盖");
    for (framework, included, excluded) in cases {
        let host = TestHost::new(framework);
        let inst = instance(framework, "instance-a");
        for (relative, text) in *included {
            host.put(&format!("{}/{relative}", inst.install_dir), text.as_bytes());
        }
        for relative in *excluded {
            host.put(
                &format!("{}/{relative}", inst.install_dir),
                b"program or runtime data",
            );
        }
        let adapter = registry.get(&inst.framework_id).unwrap();
        let captured = capture_config_backup(host.as_ref(), adapter.as_ref(), &inst)
            .await
            .unwrap();
        assert_eq!(
            captured.files.len(),
            included.len(),
            "{framework}: {:?}",
            captured.files
        );
        for (relative, text) in *included {
            assert!(
                captured
                    .files
                    .iter()
                    .any(|f| f.relative_path == *relative && f.text == *text),
                "{framework}/{relative}"
            );
        }
        for relative in *excluded {
            assert!(
                !captured.files.iter().any(|f| f.relative_path == *relative),
                "{framework}/{relative}"
            );
        }
    }
}

#[test]
fn preview_rejects_escaping_program_duplicate_and_invalid_syntax_paths() {
    for path in [
        "../.env",
        "/.env",
        "C:/config.json",
        "@karinjs/config/../../secret.json",
        "@karinjs/config\\escape.json",
        "@karinjs/config/con.json",
        "@karinjs/config/x.json ",
        "bot.py",
        "node_modules/config.json",
    ] {
        assert!(
            backup("karin", "instance-a", &[(path, "{}")])
                .validate()
                .is_err(),
            "{path}"
        );
    }
    assert!(backup("unknown", "instance-a", &[]).validate().is_err());
    assert!(backup("karin", "../escape", &[]).validate().is_err());
    assert!(
        backup("karin", "instance-a", &[("@karinjs/config/x.json", "{")])
            .validate()
            .is_err()
    );
    assert!(
        backup(
            "maibot",
            "instance-a",
            &[("config/model_config.toml", "[broken")]
        )
        .validate()
        .is_err()
    );
    assert!(
        backup("koishi", "instance-a", &[("koishi.yml", "a: [")])
            .validate()
            .is_err()
    );
    assert!(
        backup(
            "karin",
            "instance-a",
            &[
                ("@karinjs/config/a.json", "{}"),
                ("@karinjs/config/A.json", "{}")
            ]
        )
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn rejects_symlinks_at_root_config_directory_and_file() {
    let registry = crate::AppFrameworkRegistry::with_builtin();
    let inst = instance("karin", "instance-a");
    let adapter = registry.get(&inst.framework_id).unwrap();
    for link in [
        "/apps/instance-a",
        "/apps/instance-a/@karinjs/config",
        "/apps/instance-a/@karinjs/config/adapter.json",
    ] {
        let host = TestHost::new("remote");
        host.put("/apps/instance-a/@karinjs/config/adapter.json", b"{}");
        host.nodes.lock().unwrap().get_mut(link).unwrap().symlink = true;
        assert!(
            capture_config_backup(host.as_ref(), adapter.as_ref(), &inst)
                .await
                .is_err(),
            "{link}"
        );
        let mut plan = FrameworkConfigRestorePlan::default();
        assert!(
            plan.add(
                host,
                &inst,
                &backup(
                    "karin",
                    "instance-a",
                    &[("@karinjs/config/adapter.json", "{}")]
                )
            )
            .await
            .is_err(),
            "{link}"
        );
    }
}

#[test]
fn configuration_budget_is_checked_before_restore() {
    let text = "a".repeat(MAX_CONFIG_FILE_BYTES + 1);
    assert!(
        backup("nonebot2", "instance-a", &[(".env", &text)])
            .validate()
            .is_err()
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_existing_alias_is_checked_even_when_not_listed_by_long_name() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("existing-project");
    std::fs::create_dir(&directory).unwrap();
    let host = ncd_host::local::LocalWindowsHost::new();
    let parent = HostPath::from_windows(temporary.path().to_str().unwrap());
    let entry = lookup_entry(&host, &parent, "existing-project", &[])
        .await
        .unwrap()
        .unwrap();
    assert!(entry.is_dir);
    assert!(!entry.is_symlink);
    assert!(
        checked_install_root(&host, &parent.join("existing-project"))
            .await
            .unwrap()
    );
    assert!(
        lookup_entry(&host, &parent, "missing-project", &[])
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn failed_write_rolls_back_earlier_hosts_partial_write_and_new_files() {
    let first = TestHost::new("first");
    let second = TestHost::new("second");
    first.put("/apps/a/.env", b"original\r\n");
    second.mkdir("/apps/b");
    let mut plan = FrameworkConfigRestorePlan::default();
    plan.add(
        first.clone(),
        &instance("nonebot2", "a"),
        &backup("nonebot2", "a", &[(".env", "updated"), (".env.dev", "new")]),
    )
    .await
    .unwrap();
    plan.add(
        second.clone(),
        &instance("nonebot2", "b"),
        &backup("nonebot2", "b", &[(".env", "new second")]),
    )
    .await
    .unwrap();
    *second.fail_once.lock().unwrap() = Some("/apps/b/.env".into());
    let committed = std::sync::atomic::AtomicBool::new(false);
    let error = plan
        .commit_with(async {
            committed.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();
    assert!(error.contains("已回滚"));
    assert!(!committed.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(first.content("/apps/a/.env").unwrap(), b"original\r\n");
    assert!(first.content("/apps/a/.env.dev").is_none());
    assert!(second.content("/apps/b/.env").is_none());
}

#[tokio::test]
async fn failed_desktop_commit_restores_original_bytes_on_every_host() {
    let first = TestHost::new("first");
    let second = TestHost::new("second");
    first.put("/apps/a/.env", &[0xff, 0xfe, 1]);
    second.put("/apps/b/.env", b"B=old");
    let mut plan = FrameworkConfigRestorePlan::default();
    plan.add(
        first.clone(),
        &instance("nonebot2", "a"),
        &backup("nonebot2", "a", &[(".env", "A=new")]),
    )
    .await
    .unwrap();
    plan.add(
        second.clone(),
        &instance("nonebot2", "b"),
        &backup("nonebot2", "b", &[(".env", "B=new")]),
    )
    .await
    .unwrap();
    let error = plan
        .commit_with(async { Err::<(), _>("desktop transaction failed".into()) })
        .await
        .unwrap_err();
    assert!(error.contains("desktop transaction failed"));
    assert_eq!(first.content("/apps/a/.env").unwrap(), [0xff, 0xfe, 1]);
    assert_eq!(second.content("/apps/b/.env").unwrap(), b"B=old");
}

#[tokio::test]
async fn changed_target_is_rejected_before_any_mutation() {
    let host = TestHost::new("first");
    host.put("/apps/a/.env", b"before");
    let mut plan = FrameworkConfigRestorePlan::default();
    plan.add(
        host.clone(),
        &instance("nonebot2", "a"),
        &backup("nonebot2", "a", &[(".env", "new")]),
    )
    .await
    .unwrap();
    host.put("/apps/a/.env", b"edited meanwhile");
    assert!(
        plan.commit_with(async { Ok(()) })
            .await
            .unwrap_err()
            .contains("已变更")
    );
    assert_eq!(host.content("/apps/a/.env").unwrap(), b"edited meanwhile");
}

#[tokio::test]
async fn successful_restore_preserves_text_and_does_not_touch_other_files() {
    let host = TestHost::new("first");
    host.put("/apps/a/data/cmd_config.json", b"{}");
    host.put("/apps/a/data/data_v4.db", b"history");
    let text = "\u{feff}{\"custom_unknown\": [1, 2]}\r\n";
    let mut plan = FrameworkConfigRestorePlan::default();
    plan.add(
        host.clone(),
        &instance("astrbot", "a"),
        &backup(
            "astrbot",
            "a",
            &[
                ("data/cmd_config.json", text),
                ("data/config/test_config.json", "{}"),
            ],
        ),
    )
    .await
    .unwrap();
    assert_eq!(plan.commit_with(async { Ok(42) }).await.unwrap(), 42);
    assert_eq!(
        host.content("/apps/a/data/cmd_config.json").unwrap(),
        text.as_bytes()
    );
    assert_eq!(host.content("/apps/a/data/data_v4.db").unwrap(), b"history");
    assert_eq!(
        host.content("/apps/a/data/config/test_config.json")
            .unwrap(),
        b"{}"
    );
}

#[tokio::test]
async fn mismatched_instance_and_overlapping_destinations_are_rejected() {
    let host = TestHost::new("first");
    host.mkdir("/apps/a");
    let mut plan = FrameworkConfigRestorePlan::default();
    assert!(
        plan.add(
            host.clone(),
            &instance("karin", "a"),
            &backup("nonebot2", "a", &[(".env", "A=1")])
        )
        .await
        .is_err()
    );
    plan.add(
        host.clone(),
        &instance("nonebot2", "a"),
        &backup("nonebot2", "a", &[(".env", "A=1")]),
    )
    .await
    .unwrap();
    let mut same_directory = instance("nonebot2", "b");
    same_directory.install_dir = "/apps/a".into();
    assert!(
        plan.add(
            host,
            &same_directory,
            &backup("nonebot2", "b", &[(".env", "B=1")])
        )
        .await
        .is_err()
    );
}
