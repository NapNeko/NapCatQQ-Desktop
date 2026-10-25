use super::*;
use ncd_domain::bot_config::{
    AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig, DeploymentType,
};

fn config(backend: BackendType, mode: Option<SnowLumaStartMode>) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: "managed-qq-test".into(),
            qq_id: 100200,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: backend,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: mode,
            webui_password_takeover: false,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
        docker: None,
        status_command: None,
    }
}

fn prepare_runtime(root: &Path) -> FileSystemRuntimeLaunchPlanner {
    let components = root.join("NCD 数据").join("components");
    let napcat = components.join("NapCatQQ");
    let snowluma = components.join("SnowLuma");
    std::fs::create_dir_all(&napcat).unwrap();
    std::fs::create_dir_all(&snowluma).unwrap();
    for name in [
        "NapCatWinBootMain.exe",
        "NapCatWinBootHook.dll",
        "napcat.mjs",
        "qqnt.json",
    ] {
        std::fs::write(napcat.join(name), b"fixture").unwrap();
    }
    std::fs::write(snowluma.join("node.exe"), b"fixture").unwrap();
    FileSystemRuntimeLaunchPlanner::new(&components)
        .with_snowluma_runtime_root(snowluma)
        .with_snowluma_data_root(root.join("snowluma-data"))
}

fn activate_qq(planner: &FileSystemRuntimeLaunchPlanner, version: &str) -> PathBuf {
    let qq_root = ncd_component::managed_qq_root(&planner.runtime_root);
    let installed = qq_root.join("versions").join(version);
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(installed.join("QQ.exe"), b"fixture").unwrap();
    std::fs::write(qq_root.join("current.txt"), format!("{version}\n")).unwrap();
    installed
}

#[tokio::test]
async fn napcat_launch_uses_managed_qq_without_relying_on_the_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let planner = prepare_runtime(tmp.path());
    let qq = activate_qq(&planner, "9.9.36-53644");
    let bot_id = BotId::from("100200");
    let cfg = config(BackendType::NapCat, None);
    let RuntimeLaunchPlan::NapCat(plan) = planner.build_plan(&bot_id, &cfg).await.unwrap() else {
        panic!("expected NapCat plan");
    };
    assert_eq!(PathBuf::from(&plan.args[0]), qq.join("QQ.exe"));
    assert_eq!(
        plan.program,
        planner.runtime_root.join("NapCatQQ/NapCatWinBootMain.exe")
    );
    assert_eq!(
        local_qq_source(&planner.runtime_root),
        ncd_domain::LocalQqSource::Managed
    );
}

#[tokio::test]
async fn snowluma_default_cold_start_passes_the_managed_qq_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let planner = prepare_runtime(tmp.path());
    let qq = activate_qq(&planner, "9.9.36-53644");
    let bot_id = BotId::from("100200");
    let cfg = config(BackendType::SnowLuma, None);
    let plan = planner.build_plan(&bot_id, &cfg).await.unwrap();
    let runtime = plan.into_runtime_config(BotRuntimeConfig::default_path(tmp.path(), bot_id));
    assert_eq!(
        PathBuf::from(&runtime.environment["SNOWLUMA_QQ_EXE"]),
        qq.join("QQ.exe")
    );
    assert_eq!(runtime.environment["SNOWLUMA_START_MODE"], "cold_start");
}

#[tokio::test]
async fn existing_planner_reads_the_current_version_again_for_each_start() {
    let tmp = tempfile::tempdir().unwrap();
    let planner = prepare_runtime(tmp.path());
    let bot_id = BotId::from("100200");
    let cfg = config(BackendType::NapCat, None);
    activate_qq(&planner, "9.9.35-old");
    planner.build_plan(&bot_id, &cfg).await.unwrap();
    let latest = activate_qq(&planner, "9.9.36-new");
    let RuntimeLaunchPlan::NapCat(plan) = planner.build_plan(&bot_id, &cfg).await.unwrap() else {
        panic!("expected NapCat plan");
    };
    assert_eq!(PathBuf::from(&plan.args[0]), latest.join("QQ.exe"));
}

#[tokio::test]
async fn snowluma_hot_start_does_not_force_the_managed_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let planner = prepare_runtime(tmp.path());
    activate_qq(&planner, "9.9.36-53644");
    let cfg = config(BackendType::SnowLuma, Some(SnowLumaStartMode::HotStart));
    let RuntimeLaunchPlan::SnowLuma(plan) = planner
        .build_plan(&BotId::from("100200"), &cfg)
        .await
        .unwrap()
    else {
        panic!("expected SnowLuma plan");
    };
    assert!(plan.qq_install_path.is_none());
}

#[test]
fn startup_file_checks_reject_a_directory_named_qq_exe() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("QQ.exe");
    std::fs::create_dir(&path).unwrap();
    assert!(matches!(
        ensure_runtime_file(&path, "QQ missing"),
        Err(RuntimeLaunchPlanError::MissingFile { .. })
    ));
}
