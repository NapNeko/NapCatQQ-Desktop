//! 适配器层：停着的实例改 koishi.yml（对接、商店启停、类型化保存），跑着的实例交给控制台

use super::*;
use ncd_domain::{AppFrameworkId, AppInstanceId, AppPlacement};

fn instance(dir: &str, state: AppInstanceState) -> AppInstance {
    AppInstance {
        id: AppInstanceId::new("ko1"),
        framework_id: AppFrameworkId::new("koishi"),
        display_name: "Koishi".into(),
        placement: AppPlacement::LocalNative,
        host_id: "local".into(),
        install_dir: dir.into(),
        port: 5140,
        state,
        link: None,
        installed_version: None,
        last_error: None,
        created_at_ms: 0,
        install_renderer: false,
        origin: ncd_domain::AppInstanceOrigin::Created,
        auto_start: true,
    }
}

#[test]
fn documents_list_koishi_yml_first_as_yaml() {
    let docs = koishi_config_documents();
    assert_eq!(docs[0].rel_path, "koishi.yml");
    assert_eq!(docs[0].format, AppConfigFormat::Yaml);
    assert!(docs.iter().all(|d| !d.hot_reload));
}

#[test]
fn live_port_only_for_running_and_needs_registration_remotely() {
    let a = KoishiAdapter::new();
    let stopped = instance("/x", AppInstanceState::Installed);
    assert_eq!(a.live_port(&stopped).unwrap(), None);
    let running = instance("/x", AppInstanceState::Running);
    assert_eq!(
        a.live_port(&running).unwrap(),
        Some(5140),
        "本机直接用实例口"
    );
    let mut remote = running.clone();
    remote.placement = AppPlacement::RemoteNative;
    remote.host_id = "remote:s1".into();
    assert!(a.live_port(&remote).is_err());
    a.note_live_port("ko1", 41000);
    assert_eq!(a.live_port(&remote).unwrap(), Some(41000));
}

#[test]
fn link_parent_prefers_enabled_adapter_group() {
    let cfg = KoishiInstanceConfig::parse("plugins:\n  group:adapter: {}\n").unwrap();
    assert_eq!(KoishiAdapter::link_parent(&cfg), "adapter");
    let off = KoishiInstanceConfig::parse("plugins:\n  ~group:adapter: {}\n").unwrap();
    assert_eq!(KoishiAdapter::link_parent(&off), "");
}

#[cfg(windows)]
mod local {
    use super::*;
    use ncd_host::local::LocalWindowsHost;

    const MIGRATED: &str = include_str!("testdata/koishi.migrated.yml");

    fn setup() -> (tempfile::TempDir, AppInstance) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("koishi.yml"), MIGRATED).unwrap();
        let path = HostPath::from_windows(&dir.path().to_string_lossy());
        let inst = instance(path.as_posix(), AppInstanceState::Installed);
        (dir, inst)
    }

    fn plan(qq: &str) -> OneBotLinkPlan {
        use ncd_domain::{
            AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig,
            ConnectConfig, DeploymentType, RuntimeTarget,
        };
        let bot = BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id: qq.parse().unwrap(),
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::default(),
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        };
        let inst = instance("/x", AppInstanceState::Installed);
        KoishiIntegration::new()
            .plan_link(&inst, &bot, "tok")
            .unwrap()
    }

    #[tokio::test]
    async fn apply_then_unlink_edits_adapter_group_on_disk() {
        let (dir, inst) = setup();
        let host = LocalWindowsHost::new();
        let a = KoishiAdapter::new();
        a.apply_link(&host, &inst, &plan("10001")).await.unwrap();
        let text = std::fs::read_to_string(dir.path().join("koishi.yml")).unwrap();
        let cfg = KoishiInstanceConfig::parse(&text).unwrap();
        assert_eq!(
            cfg.parent_ident_of("adapter-onebot", "ncd-link").as_deref(),
            Some("adapter")
        );
        let node = cfg.find("adapter-onebot", "ncd-link").unwrap();
        assert!(node.enabled);
        assert_eq!(node.config["selfId"], "10001");
        assert_eq!(
            a.read_access_token(&host, &inst).await.unwrap().as_deref(),
            Some("tok")
        );

        // 换 Bot：同一条改 selfId，不新增
        a.apply_link(&host, &inst, &plan("20002")).await.unwrap();
        let cfg = KoishiInstanceConfig::parse(
            &std::fs::read_to_string(dir.path().join("koishi.yml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            cfg.walk()
                .iter()
                .filter(|n| n.name == "adapter-onebot")
                .count(),
            1
        );
        assert_eq!(
            cfg.find("adapter-onebot", "ncd-link").unwrap().config["selfId"],
            "20002"
        );

        a.unlink(&host, &inst).await.unwrap();
        let text = std::fs::read_to_string(dir.path().join("koishi.yml")).unwrap();
        assert!(text.contains("~adapter-onebot:ncd-link"), "{text}");
        assert!(dir.path().join("koishi.yml.ncd.bak").exists());
    }

    #[tokio::test]
    async fn typed_write_round_trips_and_reports_port() {
        let (dir, inst) = setup();
        let host = LocalWindowsHost::new();
        let a = KoishiAdapter::new();
        let env = a.read_config(&host, &inst).await.unwrap();
        let AppInstanceConfig::Koishi(mut cfg) = env.config.clone() else {
            panic!("expected koishi");
        };
        assert_eq!(env.config.listen_port(), 5140);
        cfg.pin_server(23140);
        cfg.global.insert("prefix".into(), serde_json::json!(["#"]));
        let out = a
            .write_config(&host, &inst, &AppInstanceConfig::Koishi(cfg))
            .await
            .unwrap();
        assert_eq!(out.config.listen_port(), 23140);
        assert_ne!(out.revision, env.revision);
        let text = std::fs::read_to_string(dir.path().join("koishi.yml")).unwrap();
        assert!(text.contains("prefix"));
        assert!(!text.contains("maxPort"));
    }

    #[tokio::test]
    async fn store_enable_creates_or_toggles_nodes() {
        let (dir, inst) = setup();
        let host = LocalWindowsHost::new();
        let a = KoishiAdapter::new();
        a.set_store_enabled(
            &host,
            &inst,
            "koishi-plugin-foo",
            AppStoreResource::Plugin,
            true,
            false,
        )
        .await
        .unwrap();
        a.set_store_enabled(
            &host,
            &inst,
            "@koishijs/plugin-inspect",
            AppStoreResource::Plugin,
            true,
            false,
        )
        .await
        .unwrap();
        let cfg = KoishiInstanceConfig::parse(
            &std::fs::read_to_string(dir.path().join("koishi.yml")).unwrap(),
        )
        .unwrap();
        assert!(cfg.walk().iter().any(|n| n.name == "foo" && n.enabled));
        assert!(cfg.find("inspect", "wppufr").unwrap().enabled);
        assert!(
            a.set_store_enabled(
                &host,
                &inst,
                "@koishijs/plugin-server",
                AppStoreResource::Plugin,
                false,
                false
            )
            .await
            .is_err(),
            "核心插件不给停"
        );
    }

    #[tokio::test]
    async fn raw_yml_edit_is_refused_while_running() {
        let (_dir, mut inst) = setup();
        inst.state = AppInstanceState::Running;
        let host = LocalWindowsHost::new();
        let err = KoishiAdapter::new()
            .write_config_text(&host, &inst, DOC_KOISHI_YML, "plugins: {}\n", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("先停止实例"));
    }
}
