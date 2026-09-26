use super::disk::{parse_manifest, render_manifest};
use super::*;

#[test]
fn fields_follow_python_formatter() {
    let got = prompt_fields("你好 {bot_name}，{a.b} {c[0]} {d!r} {e:>5} {f:{width}}").unwrap();
    assert_eq!(got, ["bot_name", "a", "c", "d", "e", "f"]);
    assert_eq!(prompt_fields("{{literal}} {x} 例子 {{\"k\": 1}}").unwrap(), ["x"]);
    assert_eq!(prompt_fields("{}").unwrap(), [""]);
    assert!(prompt_fields("单独的 } 不行").is_err());
    assert!(prompt_fields("没收尾的 {name").is_err());
    assert!(prompt_fields("{a{b}").is_err());
}

#[test]
fn save_check_matches_upstream_and_blocks_what_breaks_startup() {
    let default = "{identity}\n{reply_style} 然后 {{JSON}}";
    assert!(check_prompt("{reply_style}{identity}{identity}", default).is_ok());
    assert_eq!(check_prompt("  \n", default).unwrap_err(), "提示词不能是空的");
    assert!(check_prompt("{identity}{reply_style}{}", default).unwrap_err().contains("空的 {}"));
    assert_eq!(
        check_prompt("{identity}{bot_name}", default).unwrap_err(),
        "少了 {reply_style}；多了 {bot_name}（麦麦给不出这些参数）"
    );
}

#[test]
fn names_are_checked_before_touching_paths() {
    assert!(check_names("zh-CN", "maisaka_replyer.prompt").is_ok());
    assert!(check_names("zh-CN", "../x.prompt").is_err());
    assert!(check_names("zh/CN", "a.prompt").is_err());
    assert!(check_names("zh-CN", "a.txt").is_err());
    assert!(check_version_id("v20260926103015-2").is_ok());
    assert!(check_version_id("..").is_err());
    assert!(check_version_id("a/b").is_err());
}

#[test]
fn manifest_round_trip_keeps_unknown_keys_and_drops_bad_rows() {
    let raw = r#"{
  "active_version_id": "v1",
  "note": "丢掉",
  "versions": [
    {"id": "v1", "label": "第一版", "created_at": 1.5, "modified_at": 2.5, "by": "我"},
    {"id": 3},
    "not an entry"
  ]
}"#;
    let want = "{\n  \"active_version_id\": \"v1\",\n  \"versions\": [\n    {\n      \"id\": \"v1\",\n      \"label\": \"第一版\",\n      \"created_at\": 1.5,\n      \"modified_at\": 2.5,\n      \"by\": \"我\"\n    }\n  ]\n}";
    assert_eq!(render_manifest(&parse_manifest(raw).unwrap()), want);
    assert!(parse_manifest("[1, 2]").is_none());
    let empty = parse_manifest(r#"{"active_version_id": 5, "versions": {}}"#).unwrap();
    assert_eq!(render_manifest(&empty), "{\n  \"active_version_id\": null,\n  \"versions\": []\n}");
}

mod on_disk {
    use ncd_domain::{AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, AppPlacement};
    use ncd_host::local::LocalWindowsHost;

    use super::super::*;

    fn instance(dir: &std::path::Path) -> AppInstance {
        AppInstance {
            id: AppInstanceId::new("m1"),
            framework_id: AppFrameworkId::new("maibot"),
            display_name: "麦麦".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: dir.to_string_lossy().replace('\\', "/"),
            port: 23001,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
            install_renderer: false,
            origin: ncd_domain::AppInstanceOrigin::Created,
            auto_start: true,
        }
    }

    fn put(dir: &std::path::Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn save(content: &str, label: &str, version_id: Option<&str>) -> MaiBotPromptAction {
        MaiBotPromptAction::Save {
            language: "zh-CN".into(),
            name: "greet.prompt".into(),
            content: content.into(),
            label: label.into(),
            version_id: version_id.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn catalog_reads_metadata_and_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        put(tmp.path(), "prompts/zh-CN/greet.prompt", "你好 {bot_name}");
        put(tmp.path(), "prompts/zh-CN/deep.prompt", "{x}");
        put(
            tmp.path(),
            "prompts/zh-CN/.meta.toml",
            "[greet]\ndisplay_name = \"问候\"\ndescription = \"打招呼\"\n[deep]\nadvanced = true\n",
        );
        put(tmp.path(), "prompts/en-US/greet.prompt", "Hi {bot_name}");
        put(tmp.path(), "data/custom_prompts/zh-CN/greet.prompt", "嗨 {bot_name}");
        let host = LocalWindowsHost::new();
        let inst = instance(tmp.path());

        let cat = catalog(MaiBotPromptTarget::Disk { host: &host, instance: &inst }).await.unwrap();
        assert!(!cat.live);
        assert_eq!(cat.languages.iter().map(|l| l.language.as_str()).collect::<Vec<_>>(), ["en-US", "zh-CN"]);
        let zh = &cat.languages[1].prompts;
        assert_eq!(zh.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["deep.prompt", "greet.prompt"]);
        assert!(zh[0].advanced && zh[0].display_name.is_empty());
        assert_eq!((zh[1].display_name.as_str(), zh[1].description.as_str()), ("问候", "打招呼"));
        // 有覆盖没清单：上游补一条旧格式版本
        assert!(zh[1].customized);
        assert_eq!(zh[1].version_count, 1);

        let f = file(MaiBotPromptTarget::Disk { host: &host, instance: &inst }, "zh-CN", "greet.prompt").await.unwrap();
        assert_eq!(f.content, "嗨 {bot_name}");
        assert_eq!(f.default_content, "你好 {bot_name}");
        assert_eq!(f.active_version_id.as_deref(), Some("legacy-current"));
    }

    #[tokio::test]
    async fn save_activate_delete_restore_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        put(tmp.path(), "prompts/zh-CN/greet.prompt", "你好\r\n{bot_name}");
        let host = LocalWindowsHost::new();
        let inst = instance(tmp.path());
        let t = || MaiBotPromptTarget::Disk { host: &host, instance: &inst };

        let fresh = file(t(), "zh-CN", "greet.prompt").await.unwrap();
        assert_eq!(fresh.content, "你好\n{bot_name}");
        assert!(!fresh.customized && fresh.versions.is_empty() && fresh.active_version_id.is_none());

        assert!(act(t(), &save("没参数", "", None)).await.is_err());

        let first = act(t(), &save("第一版 {bot_name}", "", None)).await.unwrap();
        let v1 = first.active_version_id.clone().unwrap();
        assert!(first.customized);
        assert!(first.versions[0].label.starts_with("greet 自定义版本 "));
        let on_disk = std::fs::read_to_string(tmp.path().join("data/custom_prompts/zh-CN/greet.prompt")).unwrap();
        assert_eq!(on_disk, "第一版 {bot_name}");

        let renamed = act(t(), &save("第一版改 {bot_name}", "改过", Some(&v1))).await.unwrap();
        assert_eq!(renamed.versions.len(), 1);
        assert_eq!(renamed.versions[0].label, "改过");

        // 同一秒再建一个：版本号往后加 -2
        let second = act(t(), &save("第二版 {bot_name}", "二", None)).await.unwrap();
        assert_eq!(second.versions.len(), 2);
        let v2 = second.active_version_id.clone().unwrap();
        assert_ne!(v1, v2);

        let back = act(
            t(),
            &MaiBotPromptAction::Activate { language: "zh-CN".into(), name: "greet.prompt".into(), version_id: v1.clone() },
        )
        .await
        .unwrap();
        assert_eq!(back.content, "第一版改 {bot_name}");
        assert_eq!(version(t(), "zh-CN", "greet.prompt", &v2).await.unwrap(), "第二版 {bot_name}");

        // 删掉在用的：覆盖一起没了，回到默认
        let gone = act(
            t(),
            &MaiBotPromptAction::DeleteVersion { language: "zh-CN".into(), name: "greet.prompt".into(), version_id: v1 },
        )
        .await
        .unwrap();
        assert!(!gone.customized);
        assert_eq!(gone.versions.len(), 1);
        assert!(gone.active_version_id.is_none());

        act(
            t(),
            &MaiBotPromptAction::Activate { language: "zh-CN".into(), name: "greet.prompt".into(), version_id: v2 },
        )
        .await
        .unwrap();
        let restored =
            act(t(), &MaiBotPromptAction::Restore { language: "zh-CN".into(), name: "greet.prompt".into() }).await.unwrap();
        assert!(!restored.customized);
        assert_eq!(restored.content, "你好\n{bot_name}");
        assert_eq!(restored.versions.len(), 1, "恢复默认不删版本");
        let manifest =
            std::fs::read_to_string(tmp.path().join("data/custom_prompts/zh-CN/.versions/greet/manifest.json")).unwrap();
        assert!(manifest.starts_with("{\n  \"active_version_id\": null,\n  \"versions\": ["));
    }
}
