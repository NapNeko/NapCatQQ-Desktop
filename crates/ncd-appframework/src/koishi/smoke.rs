//! 真机冒烟：对着一个已经在跑的 Koishi（设 `NCD_KOISHI_SMOKE_DIR` 为实例目录，口按 koishi.yml 读）
//! 走一遍控制台状态、插件表单、跑着对接 / 解绑，再冒充 NapCat 握一次反向 WS。
//!
//!     cargo test -p ncd-appframework koishi_live_smoke -- --ignored --nocapture

use super::*;
use ncd_domain::{AppFrameworkId, AppInstanceId, AppPlacement};

fn bot(qq: u64) -> ncd_domain::BotConfig {
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig,
        ConnectConfig, DeploymentType, RuntimeTarget,
    };
    BotConfig {
        bot: BotBasicConfig {
            name: "b".into(),
            qq_id: qq,
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
    }
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "要一个在跑的 Koishi，设 NCD_KOISHI_SMOKE_DIR 后手动跑"]
#[allow(clippy::print_stderr)] // 这个测试就是给人看的报告，靠 --nocapture 打出来
async fn koishi_live_smoke() {
    use ncd_host::local::LocalWindowsHost;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let Ok(dir) = std::env::var("NCD_KOISHI_SMOKE_DIR") else {
        eprintln!("没设 NCD_KOISHI_SMOKE_DIR，跳过");
        return;
    };
    let host = LocalWindowsHost::new();
    let path = HostPath::from_windows(&dir);
    let mut inst = AppInstance {
        id: AppInstanceId::new("smoke"),
        framework_id: AppFrameworkId::new("koishi"),
        display_name: "Koishi".into(),
        placement: AppPlacement::LocalNative,
        host_id: "local".into(),
        install_dir: path.as_posix().to_string(),
        port: 0,
        state: AppInstanceState::Running,
        link: None,
        installed_version: None,
        last_error: None,
        created_at_ms: 0,
        install_renderer: false,
        origin: ncd_domain::AppInstanceOrigin::Created,
        auto_start: false,
    };
    let a = KoishiAdapter::new();
    let (cfg, _) = a.read_yml(&host, &inst).await.unwrap();
    inst.port = cfg.listen_port();

    let status = a.status(&inst, inst.port).await;
    eprintln!("status: {status:?}");
    assert_eq!(status.gate, KoishiRuntimeGate::Ok);

    let schemas = a
        .plugin_schemas(
            &host,
            &inst,
            &["".into(), "adapter-onebot".into(), "nope-x".into()],
        )
        .await
        .unwrap();
    assert!(schemas[0].schema.is_some(), "全局设置 schema");
    assert!(schemas[1].schema.is_some(), "{:?}", schemas[1].error);
    assert!(schemas[2].error.is_some());
    let pkgs = a.installed_packages(&host, &inst).await.unwrap();
    eprintln!("packages: {}", pkgs.len());
    assert!(
        pkgs.iter()
            .any(|p| p.package == "koishi-plugin-adapter-onebot")
    );

    let plan = KoishiIntegration::new()
        .plan_link(&inst, &bot(10001), "tok")
        .unwrap();
    a.apply_link(&host, &inst, &plan).await.unwrap();
    let (after, _) = a.read_yml(&host, &inst).await.unwrap();
    let node = after
        .find("adapter-onebot", "ncd-link")
        .expect("对接条目落盘");
    assert!(node.enabled);

    // 冒充 NapCat：带对的 X-Self-ID 应该握手成功
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let url = format!("ws://127.0.0.1:{}/onebot/ncd", inst.port);
    let mut req = url.into_client_request().unwrap();
    req.headers_mut()
        .insert("X-Self-ID", "10001".parse().unwrap());
    req.headers_mut()
        .insert("X-Client-Role", "Universal".parse().unwrap());
    let (ws, resp) = tokio_tungstenite::connect_async(req).await.unwrap();
    eprintln!("handshake: {}", resp.status());
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    let status = a.status(&inst, inst.port).await;
    eprintln!("bots after link: {:?}", status.bots);
    drop(ws);
    assert!(status.bots.iter().any(|b| b.self_id == "10001"));

    a.unlink(&host, &inst).await.unwrap();
    let (after, _) = a.read_yml(&host, &inst).await.unwrap();
    assert!(!after.find("adapter-onebot", "ncd-link").unwrap().enabled);
}
