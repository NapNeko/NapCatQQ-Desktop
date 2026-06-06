use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use ncd_runtime::{
    AdvancedConfig, AutoRestartSchedule, BackendKind, BackendType, BotActorState, BotBackend,
    BotBackendError, BotBasicConfig, BotConfig, BotConfigRepo, BotId, BotManager, BotManagerError,
    BotRuntimeConfig, BotStartCtx, BotStatus, BroadcastEventBus, ConfigStore, ConnectConfig,
    DispatchRenderer, EventBus, EventFilter, LocalBotConfigRepo, LocalConfigStore,
    NoopOfflineNotifier, ReqwestNapCatWebUiClient, RuntimeLaunchPlan, RuntimeLaunchPlanError,
    RuntimeLaunchPlanner, SecretStore, SecretStoreImpl, SnowLumaDaemon, SnowLumaWebUiClient,
    SnowLumaWebUiClientFactory, SnowLumaWebUiError, StopMode, TailOpts, WebUiPollerSettings,
};

#[derive(Default)]
struct FakeBackend {
    running: Mutex<HashSet<BotId>>,
    fail_start: Mutex<HashSet<BotId>>,
    fail_stop: Mutex<HashSet<BotId>>,
    start_count: Mutex<std::collections::HashMap<BotId, usize>>,
    stop_count: Mutex<std::collections::HashMap<BotId, usize>>,
    stop_gate: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    last_config: Mutex<Option<BotRuntimeConfig>>,
}

impl FakeBackend {
    async fn fail_next_start(&self, bot_id: impl Into<BotId>) {
        self.fail_start.lock().await.insert(bot_id.into());
    }

    async fn fail_next_stop(&self, bot_id: impl Into<BotId>) {
        self.fail_stop.lock().await.insert(bot_id.into());
    }

    async fn start_count(&self, bot_id: impl Into<BotId>) -> usize {
        let bot_id = bot_id.into();
        self.start_count
            .lock()
            .await
            .get(&bot_id)
            .copied()
            .unwrap_or(0)
    }

    async fn stop_count(&self, bot_id: impl Into<BotId>) -> usize {
        let bot_id = bot_id.into();
        self.stop_count
            .lock()
            .await
            .get(&bot_id)
            .copied()
            .unwrap_or(0)
    }

    async fn block_next_stop(&self) -> tokio::sync::oneshot::Sender<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.stop_gate.lock().await.replace(rx);
        tx
    }
}

#[async_trait]
impl BotBackend for FakeBackend {
    fn id(&self) -> &BotId {
        static ID: std::sync::OnceLock<BotId> = std::sync::OnceLock::new();
        ID.get_or_init(|| BotId::new("fake-backend"))
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn flavor(&self) -> ncd_runtime::BotFlavor {
        ncd_runtime::BotFlavor::NapCat
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        self.last_config.lock().await.replace(ctx.config.clone());
        *self
            .start_count
            .lock()
            .await
            .entry(ctx.config.bot_id.clone())
            .or_insert(0) += 1;
        assert_ne!(ctx.config.bot_id.as_str(), "19998", "fake backend panic");
        if self.fail_start.lock().await.remove(&ctx.config.bot_id) {
            return Err(BotBackendError::Io("fake start failed".to_string()));
        }
        self.running.lock().await.insert(ctx.config.bot_id.clone());
        Ok(BotStatus::running(ctx.config.bot_id.clone(), 42, 1))
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        assert_ne!(bot_id.as_str(), "19997", "fake backend panic");
        *self
            .stop_count
            .lock()
            .await
            .entry(bot_id.clone())
            .or_insert(0) += 1;
        if let Some(rx) = self.stop_gate.lock().await.take() {
            let _ = rx.await;
        }
        if self.fail_stop.lock().await.remove(&bot_id) {
            return Err(BotBackendError::Io("fake stop failed".to_string()));
        }
        self.running.lock().await.remove(&bot_id);
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        if self.running.lock().await.contains(&bot_id) {
            Ok(BotStatus::running(bot_id, 42, 1))
        } else {
            Ok(BotStatus::stopped(bot_id))
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        Ok(())
    }

    async fn tail_log(
        &self,
        _bot_id: BotId,
        _opts: TailOpts,
    ) -> Result<ncd_runtime::LogSnapshot, BotBackendError> {
        Ok(ncd_runtime::LogSnapshot {
            lines: Vec::new(),
            total_lines: 0,
        })
    }
}

#[derive(Debug, Clone)]
struct TestLaunchPlanner;

#[derive(Debug, Clone)]
struct TestMultiLaunchPlanner;

#[async_trait]
impl RuntimeLaunchPlanner for TestMultiLaunchPlanner {
    async fn build_plan(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
        match config.bot.backend_type {
            BackendType::NapCat => TestLaunchPlanner.build_plan(bot_id, config).await,
            BackendType::SnowLuma => Ok(RuntimeLaunchPlan::SnowLuma(
                ncd_runtime::SnowLumaLaunchPlan {
                    runtime_root: std::path::PathBuf::from("test-runtime/snowluma"),
                    snowluma_data_root: std::path::PathBuf::from("test-data/snowluma"),
                    start_mode: ncd_runtime::SnowLumaStartMode::HotStart,
                    qq_install_path: None,
                    bot_qq_id: config.bot.qq_id,
                },
            )),
        }
    }
}

struct FailingSnowLumaFactory;

#[async_trait]
impl SnowLumaWebUiClientFactory for FailingSnowLumaFactory {
    async fn create(
        &self,
        _password: String,
        _port: u16,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
        Err(SnowLumaWebUiError::LoginFailed(
            "unused in bot_manager tests".to_string(),
        ))
    }
}

#[async_trait]
impl RuntimeLaunchPlanner for TestLaunchPlanner {
    async fn build_plan(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
        match config.bot.backend_type {
            BackendType::NapCat => Ok(RuntimeLaunchPlan::NapCat(ncd_runtime::NapCatLaunchPlan {
                runtime_root: std::path::PathBuf::from("test-runtime"),
                napcat_dir: std::path::PathBuf::from("test-runtime/NapCatQQ"),
                program: std::path::PathBuf::from("test-runtime/NapCatQQ/NapCatWinBootMain.exe"),
                args: vec![
                    "C:/QQ/QQ.exe".to_string(),
                    "test-runtime/NapCatQQ/NapCatWinBootHook.dll".to_string(),
                    bot_id.as_str().to_string(),
                ],
                environment: std::collections::BTreeMap::from([
                    (
                        "NAPCAT_PATCH_PACKAGE".to_string(),
                        "test-runtime/NapCatQQ/qqnt.json".to_string(),
                    ),
                    (
                        "NAPCAT_LOAD_PATH".to_string(),
                        "test-runtime/NapCatQQ/loadNapCat.js".to_string(),
                    ),
                    (
                        "NAPCAT_INJECT_PATH".to_string(),
                        "test-runtime/NapCatQQ/NapCatWinBootHook.dll".to_string(),
                    ),
                    (
                        "NAPCAT_LAUNCHER_PATH".to_string(),
                        "test-runtime/NapCatQQ/NapCatWinBootMain.exe".to_string(),
                    ),
                    (
                        "NAPCAT_MAIN_PATH".to_string(),
                        "test-runtime/NapCatQQ/napcat.mjs".to_string(),
                    ),
                ]),
                working_dir: std::path::PathBuf::from("test-runtime/NapCatQQ"),
                load_script_path: std::path::PathBuf::from("test-runtime/NapCatQQ/loadNapCat.js"),
            })),
            BackendType::SnowLuma => Err(RuntimeLaunchPlanError::SnowLumaInvalidStartMode(
                "snowluma backend not wired in test planner".to_string(),
            )),
        }
    }
}

fn bot_config(qq_id: u64, name: &str) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: name.to_string(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: ncd_runtime::RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            deployment_type: ncd_runtime::DeploymentType::Native,
            snowluma_start_mode: None,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
        status_command: None,
    }
}

fn bot_config_auto_start(qq_id: u64, name: &str) -> BotConfig {
    let mut config = bot_config(qq_id, name);
    config.advanced.auto_start = true;
    config
}

/// 默认 wiring：一个本地 `ReqwestNapCatWebUiClient` + `NoopOfflineNotifier`
/// + 默认 `WebUiPollerSettings`。测试不真正发起 WebUI 请求，仅占位填充
/// `BotManager::new` 新增的 4 个依赖（design.md §15.1）。
fn default_webui_client() -> Arc<dyn ncd_runtime::NapCatWebUiClient> {
    Arc::new(ReqwestNapCatWebUiClient::new().expect("构造默认 webui client 失败"))
}

fn default_offline_notifier() -> Arc<dyn ncd_runtime::OfflineNotifier> {
    Arc::new(NoopOfflineNotifier)
}

fn default_poller_settings() -> Arc<tokio::sync::RwLock<WebUiPollerSettings>> {
    Arc::new(tokio::sync::RwLock::new(WebUiPollerSettings::default()))
}

fn make_manager(
    root: &std::path::Path,
) -> (
    Arc<LocalConfigStore>,
    Arc<LocalBotConfigRepo<LocalConfigStore>>,
    Arc<FakeBackend>,
    BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>,
) {
    let store = Arc::new(LocalConfigStore::new(root));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(root.join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(store.config_dir(), store.config_dir()));
    let backend = Arc::new(FakeBackend::default());
    let event_bus = Arc::new(BroadcastEventBus::default());
    let planner = Arc::new(TestLaunchPlanner);
    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        backend.clone(),
        planner,
        event_bus,
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
    );
    (store, repo, backend, manager)
}

fn make_manager_with_planner(
    root: &std::path::Path,
    planner: Arc<dyn RuntimeLaunchPlanner>,
) -> (
    Arc<LocalConfigStore>,
    Arc<LocalBotConfigRepo<LocalConfigStore>>,
    Arc<FakeBackend>,
    BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>,
) {
    let store = Arc::new(LocalConfigStore::new(root));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(root.join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(store.config_dir(), store.config_dir()));
    let backend = Arc::new(FakeBackend::default());
    let event_bus = Arc::new(BroadcastEventBus::default());
    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        backend.clone(),
        planner,
        event_bus,
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
    );
    (store, repo, backend, manager)
}

fn touch(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"").unwrap();
}

fn prepare_napcat_runtime(root: &std::path::Path) {
    let napcat_dir = root.join("NapCatQQ");
    touch(&napcat_dir.join("NapCatWinBootMain.exe"));
    touch(&napcat_dir.join("NapCatWinBootHook.dll"));
    touch(&napcat_dir.join("napcat.mjs"));
    touch(&napcat_dir.join("qqnt.json"));
}

async fn build_plan_with_fake_qq(
    runtime_root: &std::path::Path,
    qq_install: &std::path::Path,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    ncd_runtime::build_napcat_launch_plan_with_qq_install_path(
        &BotId::new("10001"),
        &bot_config(10001, "bot"),
        runtime_root,
        qq_install,
    )
    .await
}

// ─── NapCat 启动计划 ───────────────────────────────────────────────────────────

#[tokio::test]
async fn napcat_launch_plan_builds_command_env_working_dir_and_load_script() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let runtime_root = temp.path().join("runtime");
    let qq_install = temp.path().join("QQNT");
    prepare_napcat_runtime(&runtime_root);
    touch(&qq_install.join("QQ.exe"));

    let plan = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap();
    let RuntimeLaunchPlan::NapCat(plan) = plan else {
        panic!("expected NapCat plan");
    };

    assert_eq!(
        plan.program,
        runtime_root.join("NapCatQQ/NapCatWinBootMain.exe")
    );
    assert_eq!(
        plan.args,
        vec![
            qq_install.join("QQ.exe").to_string_lossy().to_string(),
            runtime_root
                .join("NapCatQQ")
                .join("NapCatWinBootHook.dll")
                .to_string_lossy()
                .to_string(),
            "10001".to_string(),
        ]
    );
    assert_eq!(plan.working_dir, runtime_root.join("NapCatQQ"));
    let patch_package = runtime_root
        .join("NapCatQQ")
        .join("qqnt.json")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        plan.environment
            .get("NAPCAT_PATCH_PACKAGE")
            .map(String::as_str),
        Some(patch_package.as_str())
    );
    let load_path = plan.load_script_path.to_string_lossy().to_string();
    assert_eq!(
        plan.environment.get("NAPCAT_LOAD_PATH").map(String::as_str),
        Some(load_path.as_str())
    );

    let script = std::fs::read_to_string(plan.load_script_path).unwrap();
    assert!(script.starts_with("(async () => {await import('file:///"));
    assert!(script.contains("napcat.mjs"));
    assert!(script.ends_with("')})()"));
}

#[tokio::test]
async fn napcat_launch_plan_reports_missing_runtime_components() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let runtime_root = temp.path().join("runtime");
    let qq_install = temp.path().join("QQNT");
    touch(&qq_install.join("QQ.exe"));

    let err = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap_err();
    let err = err.to_string();
    assert!(err.contains("NapCatWinBootMain.exe"));
    assert!(err.contains("checked path:"));
    assert!(
        err.contains(
            runtime_root
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe")
                .to_string_lossy()
                .as_ref()
        )
    );

    prepare_napcat_runtime(&runtime_root);
    std::fs::remove_file(runtime_root.join("NapCatQQ/NapCatWinBootHook.dll")).unwrap();
    let err = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("NapCatWinBootHook.dll"));

    prepare_napcat_runtime(&runtime_root);
    std::fs::remove_file(runtime_root.join("NapCatQQ/napcat.mjs")).unwrap();
    let err = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("napcat.mjs"));

    prepare_napcat_runtime(&runtime_root);
    std::fs::remove_file(runtime_root.join("NapCatQQ/qqnt.json")).unwrap();
    let err = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("qqnt.json"));
}

#[tokio::test]
async fn napcat_launch_plan_reports_missing_qq_exe() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let runtime_root = temp.path().join("runtime");
    let qq_install = temp.path().join("QQNT");
    prepare_napcat_runtime(&runtime_root);

    let err = build_plan_with_fake_qq(&runtime_root, &qq_install)
        .await
        .unwrap_err();
    let err = err.to_string();
    assert!(err.contains("QQ.exe"));
    assert!(err.contains("checked path:"));
    assert!(err.contains(qq_install.join("QQ.exe").to_string_lossy().as_ref()));
}

// ─── 4 开上限 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_enforces_4_bot_limit() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    for i in 1..=4 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    assert_eq!(manager.bot_count().await, 4);

    let err = manager
        .upsert_bot_config(bot_config(10005, "bot-5"))
        .await
        .unwrap_err();

    assert!(matches!(err, BotManagerError::BotLimitReached));
    assert_eq!(manager.bot_count().await, 4);
}

#[tokio::test]
async fn upsert_existing_bot_does_not_count_toward_limit() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    for i in 1..=4 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 更新已有的 Bot 不应触发上限
    manager
        .upsert_bot_config(bot_config(10001, "bot-1-updated"))
        .await
        .unwrap();

    assert_eq!(manager.bot_count().await, 4);
}

// ─── 启停状态流转 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_and_stop_transitions() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    let bot_id = BotId::new("10001");
    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();

    // 初始状态 Stopped
    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);

    // 启动 → Running
    let snap = manager.start_bot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Running);

    // 停止 → Stopped
    let snap = manager.stop_bot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);
}

#[tokio::test]
async fn start_napcat_uses_launch_plan_instead_of_empty_command() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10010");

    manager
        .upsert_bot_config(bot_config(10010, "bot"))
        .await
        .unwrap();

    let snap = manager.start_bot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Running);

    let config = backend
        .last_config
        .lock()
        .await
        .clone()
        .expect("runtime config should be passed to backend");
    assert_eq!(
        config.launch_command,
        vec![
            "test-runtime/NapCatQQ/NapCatWinBootMain.exe".to_string(),
            "C:/QQ/QQ.exe".to_string(),
            "test-runtime/NapCatQQ/NapCatWinBootHook.dll".to_string(),
            "10010".to_string(),
        ]
    );
    assert_eq!(
        config.working_dir.as_deref(),
        Some(std::path::Path::new("test-runtime/NapCatQQ"))
    );
    assert_eq!(
        config
            .environment
            .get("NAPCAT_MAIN_PATH")
            .map(String::as_str),
        Some("test-runtime/NapCatQQ/napcat.mjs")
    );
}

#[tokio::test]
async fn start_backend_failure_marks_crashed() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10011");

    manager
        .upsert_bot_config(bot_config(10011, "bot"))
        .await
        .unwrap();
    backend.fail_next_start(bot_id.clone()).await;

    let err = manager.start_bot(&bot_id).await.unwrap_err();
    assert!(matches!(
        err,
        BotManagerError::Runtime(BotBackendError::Io(_))
    ));

    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Crashed);
    assert_eq!(
        snap.last_error.as_deref(),
        Some("io error: fake start failed")
    );
}

#[tokio::test]
async fn snowluma_start_returns_not_implemented_without_running() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10012");
    let mut config = bot_config(10012, "snowluma");
    config.bot.backend_type = BackendType::SnowLuma;

    manager.upsert_bot_config(config).await.unwrap();

    // Test planner 在 SnowLuma 分支显式返回 `SnowLumaInvalidStartMode`，
    // 模拟"SnowLuma 启动链路尚未在 BotManager wiring 内打通"的场景。
    // 在 `RuntimeLaunchPlanError` 的 `SnowLumaNotImplemented` 被移除之后，本测试
    // 仅锁定"start_bot 应当因为启动计划构造失败而把 actor 转 Crashed"这一行为。
    let err = manager.start_bot(&bot_id).await.unwrap_err();
    let err_message = err.to_string();
    assert!(
        err_message.contains("snowluma backend not wired in test planner"),
        "expected planner error to surface, got: {err_message}"
    );

    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Crashed);
    let last_error = snap.last_error.as_deref().unwrap_or_default();
    assert!(
        last_error.contains("snowluma backend not wired in test planner"),
        "expected last_error to capture planner error, got: {last_error:?}"
    );
}

#[tokio::test]
async fn start_nonexistent_bot_returns_not_found() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    let err = manager.start_bot(&BotId::new("99999")).await.unwrap_err();
    assert!(matches!(err, BotManagerError::BotNotFound(_)));
}

// ─── 批量并发 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_start_starts_multiple_bots_concurrently() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    let ids: Vec<BotId> = (1..=3)
        .map(|i| BotId::new(format!("{}", 10000 + i)))
        .collect();

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    let result = manager.batch_start(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 3);
    assert!(result.failed.is_empty());

    // 所有 bot 都进入 Running
    for id in &ids {
        let snap = manager.get_snapshot(id).await.unwrap();
        assert_eq!(snap.state, BotActorState::Running);
    }
}

#[tokio::test]
async fn batch_start_reports_partial_failure_without_blocking_others() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let ids: Vec<BotId> = ["10001", "10002", "10003"]
        .into_iter()
        .map(BotId::new)
        .collect();

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }
    backend.fail_next_start("10002").await;

    let result = manager.batch_start(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, BotId::new("10002"));
    assert!(result.failed[0].1.to_string().contains("fake start failed"));

    let running = backend.running.lock().await;
    assert!(running.contains(&BotId::new("10001")));
    assert!(running.contains(&BotId::new("10003")));
    assert!(!running.contains(&BotId::new("10002")));
}

#[tokio::test]
async fn batch_start_reports_join_error_for_panicking_task() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());
    let ids: Vec<BotId> = ["10001", "19998"].into_iter().map(BotId::new).collect();

    manager
        .upsert_bot_config(bot_config(10001, "bot-ok"))
        .await
        .unwrap();
    manager
        .upsert_bot_config(bot_config(19998, "bot-panic"))
        .await
        .unwrap();

    let result = manager.batch_start(&ids).await.unwrap();
    assert_eq!(result.succeeded, vec![BotId::new("10001")]);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, BotId::new("19998"));
    assert!(matches!(
        result.failed[0].1,
        BotManagerError::TaskJoinFailed(_)
    ));
}

#[tokio::test]
async fn batch_start_reports_napcat_missing_runtime_component() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let planner = Arc::new(ncd_runtime::FileSystemRuntimeLaunchPlanner::new(
        temp.path().join("runtime"),
    ));
    let (_, _, _, manager) = make_manager_with_planner(temp.path(), planner);
    let bot_id = BotId::new("10020");

    manager
        .upsert_bot_config(bot_config(10020, "bot"))
        .await
        .unwrap();

    let result = manager.batch_start(&[bot_id.clone()]).await.unwrap();
    assert!(result.succeeded.is_empty());
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, bot_id);
    assert!(
        result.failed[0]
            .1
            .to_string()
            .contains("NapCatWinBootMain.exe")
    );

    let snap = manager.get_snapshot(&BotId::new("10020")).await.unwrap();
    assert_eq!(snap.state, BotActorState::Crashed);
}

#[tokio::test]
async fn batch_stop_stops_running_bots() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    let ids: Vec<BotId> = (1..=2)
        .map(|i| BotId::new(format!("{}", 10000 + i)))
        .collect();

    for i in 1..=2u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 先启动
    manager.batch_start(&ids).await.unwrap();

    // 再停止
    let result = manager.batch_stop(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert!(result.failed.is_empty());

    for id in &ids {
        let snap = manager.get_snapshot(id).await.unwrap();
        assert_eq!(snap.state, BotActorState::Stopped);
    }
}

#[tokio::test]
async fn batch_stop_reports_partial_failure_without_blocking_others() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let ids: Vec<BotId> = ["10001", "10002", "10003"]
        .into_iter()
        .map(BotId::new)
        .collect();

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }
    manager.batch_start(&ids).await.unwrap();
    backend.fail_next_stop("10002").await;

    let result = manager.batch_stop(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, BotId::new("10002"));
    assert!(result.failed[0].1.to_string().contains("fake stop failed"));

    let running = backend.running.lock().await;
    assert!(!running.contains(&BotId::new("10001")));
    assert!(running.contains(&BotId::new("10002")));
    assert!(!running.contains(&BotId::new("10003")));
}

#[tokio::test]
async fn batch_stop_reports_join_error_for_panicking_task() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let ids: Vec<BotId> = ["10001", "19997"].into_iter().map(BotId::new).collect();

    manager
        .upsert_bot_config(bot_config(10001, "bot-ok"))
        .await
        .unwrap();
    manager
        .upsert_bot_config(bot_config(19997, "bot-panic"))
        .await
        .unwrap();
    backend.running.lock().await.extend(ids.iter().cloned());

    let result = manager.batch_stop(&ids).await.unwrap();
    assert_eq!(result.succeeded, vec![BotId::new("10001")]);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, BotId::new("19997"));
    assert!(matches!(
        result.failed[0].1,
        BotManagerError::TaskJoinFailed(_)
    ));
}

// ─── 配置变更热推送 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_running_bot_keeps_running_and_writes_config() {
    // M6.5 行为变更：同 backend 同 flavor 的运行中保存配置不再重启 bot，
    // 改走 WebUI 热推送。本测试用 FakeBackend 跑流程，napcat_endpoints 表里
    // 不会有任何条目（端点表只在收到 NapCatWebuiAvailable 事件时才填），
    // 因此热推送分支会落到 "config_saved_pending_reload"，bot 状态保持 Running。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    let bot_id = BotId::new("10001");
    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();

    // 启动 → Running
    manager.start_bot(&bot_id).await.unwrap();
    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Running);

    let snap = manager
        .upsert_bot_config(bot_config(10001, "bot-updated"))
        .await
        .unwrap();

    // bot 仍在跑，没有触发重启
    assert_eq!(snap.state, BotActorState::Running);
    assert!(!snap.pending_restart);

    // 新配置必须已经写入持久化层
    let cfg = manager
        .get_bot_config(&bot_id)
        .await
        .unwrap()
        .expect("config persisted");
    assert_eq!(cfg.bot.name, "bot-updated");
}

#[tokio::test]
async fn upsert_stopped_bot_does_not_restart() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();

    // Bot 是 Stopped，更新配置不应触发 restart
    let snap = manager
        .upsert_bot_config(bot_config(10001, "bot-updated"))
        .await
        .unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);
}

// ─── 自动启动 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bootstrap_auto_starts_marked_bots() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    // 先通过 repo 直接写入配置（模拟已有持久化数据）
    repo.upsert(bot_config_auto_start(10001, "auto-bot"))
        .await
        .unwrap();
    repo.upsert(bot_config(10002, "manual-bot")).await.unwrap();

    let result = manager.bootstrap().await.unwrap();

    // 没有超出上限的 bot
    assert!(result.skipped.is_empty());

    // 只有 auto_start=true 的 bot 被启动
    assert_eq!(result.started.succeeded.len(), 1);
    assert_eq!(result.started.succeeded[0], BotId::new("10001"));

    // 两个 bot 都被注册为 actor
    assert_eq!(manager.bot_count().await, 2);

    // auto-bot 进入 Running
    let snap = manager.get_snapshot(&BotId::new("10001")).await.unwrap();
    assert_eq!(snap.state, BotActorState::Running);

    // manual-bot 保持 Stopped
    let snap = manager.get_snapshot(&BotId::new("10002")).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);
}

#[tokio::test]
async fn bootstrap_respects_4_bot_limit_and_reports_skipped() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    for i in 1..=6u64 {
        repo.upsert(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    let result = manager.bootstrap().await.unwrap();

    // 最多只注册 4 个 actor
    assert_eq!(manager.bot_count().await, 4);

    // 超出的 2 个 bot 被记入 skipped
    assert_eq!(result.skipped.len(), 2);
    assert_eq!(result.skipped[0], BotId::new("10005"));
    assert_eq!(result.skipped[1], BotId::new("10006"));
}

// ─── 删除 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_bot_removes_actor_and_config() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();
    assert_eq!(manager.bot_count().await, 1);

    manager
        .delete_bot_config(&BotId::new("10001"))
        .await
        .unwrap();

    assert_eq!(manager.bot_count().await, 0);
    assert_eq!(repo.get(10001).await.unwrap(), None);
}

#[tokio::test]
async fn batch_delete_stops_and_removes_bots() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 启动前两个
    let ids: Vec<BotId> = vec![BotId::new("10001"), BotId::new("10002")];
    manager.batch_start(&ids).await.unwrap();

    // 批量删除所有 3 个
    let all_ids: Vec<BotId> = (1..=3)
        .map(|i| BotId::new(format!("{}", 10000 + i)))
        .collect();
    let result = manager.batch_delete(&all_ids).await.unwrap();

    assert_eq!(result.succeeded.len(), 3);
    assert!(result.failed.is_empty());
    assert_eq!(manager.bot_count().await, 0);
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn delete_running_bot_calls_backend_stop_before_repo_delete() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10101");

    manager
        .upsert_bot_config(bot_config(10101, "bot"))
        .await
        .unwrap();
    manager.start_bot(&bot_id).await.unwrap();
    backend.fail_next_stop(bot_id.clone()).await;

    let err = manager.delete_bot_config(&bot_id).await.unwrap_err();
    assert!(err.to_string().contains("fake stop failed"));
    assert_eq!(backend.stop_count(bot_id.clone()).await, 1);
    assert!(repo.get(10101).await.unwrap().is_some());
}

#[tokio::test]
async fn delete_stop_failure_keeps_config_and_actor() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10102");

    manager
        .upsert_bot_config(bot_config(10102, "bot"))
        .await
        .unwrap();
    manager.start_bot(&bot_id).await.unwrap();
    backend.fail_next_stop(bot_id.clone()).await;

    let err = manager.delete_bot_config(&bot_id).await.unwrap_err();
    assert!(matches!(
        err,
        BotManagerError::Runtime(BotBackendError::Io(_))
    ));
    assert!(repo.get(10102).await.unwrap().is_some());
    assert_eq!(manager.bot_count().await, 1);
    assert!(manager.get_snapshot(&bot_id).await.is_ok());
}

#[tokio::test]
async fn delete_running_bot_stops_backend_then_removes_actor_and_config() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10103");

    manager
        .upsert_bot_config(bot_config(10103, "bot"))
        .await
        .unwrap();
    manager.start_bot(&bot_id).await.unwrap();

    manager.delete_bot_config(&bot_id).await.unwrap();

    assert_eq!(backend.stop_count(bot_id.clone()).await, 1);
    assert_eq!(repo.get(10103).await.unwrap(), None);
    assert_eq!(manager.bot_count().await, 0);
}

#[tokio::test]
async fn restart_running_docker_or_config_routes_to_config_backend() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, napcat_backend, manager) =
        make_manager_with_planner(temp.path(), Arc::new(TestMultiLaunchPlanner));
    let snowluma_backend = Arc::new(FakeBackend::default());
    let event_bus = Arc::new(BroadcastEventBus::default());
    let daemon = SnowLumaDaemon::new(
        temp.path().join("snowluma-data"),
        temp.path().join("snowluma-runtime"),
        event_bus,
        Arc::new(FailingSnowLumaFactory),
    );
    let manager = manager.with_snowluma(snowluma_backend.clone(), daemon);
    let bot_id = BotId::new("10104");
    let mut config = bot_config(10104, "snowluma");
    config.bot.backend_type = BackendType::SnowLuma;

    manager.upsert_bot_config(config).await.unwrap();
    manager.start_bot(&bot_id).await.unwrap();
    manager.restart_bot(&bot_id).await.unwrap();

    assert_eq!(snowluma_backend.stop_count(bot_id.clone()).await, 1);
    assert_eq!(snowluma_backend.start_count(bot_id.clone()).await, 2);
    assert_eq!(napcat_backend.stop_count(bot_id).await, 0);
}

#[tokio::test]
async fn duplicate_start_does_not_call_backend_twice() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10105");

    manager
        .upsert_bot_config(bot_config(10105, "bot"))
        .await
        .unwrap();

    manager.start_bot(&bot_id).await.unwrap();
    manager.start_bot(&bot_id).await.unwrap();
    let batch = manager
        .batch_start(&[bot_id.clone(), bot_id.clone(), bot_id.clone()])
        .await
        .unwrap();

    assert_eq!(backend.start_count(bot_id).await, 1);
    assert_eq!(batch.succeeded.len(), 1);
    assert!(batch.failed.is_empty());
}

#[tokio::test]
async fn stopping_restart_does_not_start_before_stop() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot_id = BotId::new("10106");

    manager
        .upsert_bot_config(bot_config(10106, "bot"))
        .await
        .unwrap();
    manager.start_bot(&bot_id).await.unwrap();
    let release_stop = backend.block_next_stop().await;
    let stop_manager = manager.clone();
    let stop_bot_id = bot_id.clone();
    let stop_task = tokio::spawn(async move { stop_manager.stop_bot(&stop_bot_id).await });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if backend.stop_count(bot_id.clone()).await == 1
                && manager.get_snapshot(&bot_id).await.unwrap().state == BotActorState::Stopping
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let restart_snapshot = manager.restart_bot(&bot_id).await.unwrap();
    assert_eq!(restart_snapshot.state, BotActorState::Stopping);
    assert_eq!(backend.start_count(bot_id.clone()).await, 1);

    release_stop.send(()).unwrap();
    let stopped_then_started = stop_task.await.unwrap().unwrap();
    assert_eq!(stopped_then_started.state, BotActorState::Running);
    assert_eq!(backend.start_count(bot_id).await, 2);
}

// ─── 事件广播 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_bot_publishes_state_change_event() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let store = Arc::new(LocalConfigStore::new(temp.path()));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(temp.path().join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(
        temp.path().join("runtime").join("config"),
        temp.path().join("runtime").join("config"),
    ));
    let backend = Arc::new(FakeBackend::default());
    let event_bus = Arc::new(BroadcastEventBus::default());

    // 先订阅，再操作
    let mut sub = event_bus.subscribe(EventFilter::bot("10001"));

    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        backend,
        Arc::new(TestLaunchPlanner),
        Arc::clone(&event_bus),
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
    );

    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();
    manager.start_bot(&BotId::new("10001")).await.unwrap();

    // 应该收到 bot_created 和 start_requested 两个事件
    let event1 = sub.next().await.expect("expected bot_created event");
    let event2 = sub.next().await.expect("expected start_requested event");

    // 验证事件属于正确的 bot
    assert_eq!(event1.bot_id().unwrap().as_str(), "10001");
    assert_eq!(event2.bot_id().unwrap().as_str(), "10001");
}

// ─── list_snapshots ───────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_backend_switch_cleans_old_backend_files() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());
    let config_dir = temp.path().join("runtime").join("config");

    let mut napcat_config = bot_config(10008, "bot");
    napcat_config.bot.backend_type = BackendType::NapCat;
    manager.upsert_bot_config(napcat_config).await.unwrap();
    assert!(config_dir.join("onebot11_10008.json").exists());
    assert!(config_dir.join("napcat_10008.json").exists());
    assert!(!config_dir.join("onebot_10008.json").exists());

    let mut snowluma_config = bot_config(10008, "bot");
    snowluma_config.bot.backend_type = BackendType::SnowLuma;
    manager.upsert_bot_config(snowluma_config).await.unwrap();

    assert!(config_dir.join("onebot_10008.json").exists());
    assert!(!config_dir.join("onebot11_10008.json").exists());
    assert!(!config_dir.join("napcat_10008.json").exists());
}

#[tokio::test]
async fn list_snapshots_returns_all_actors() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = make_manager(temp.path());

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    let snapshots = manager.list_snapshots().await;
    assert_eq!(snapshots.len(), 3);
}

// ─── 持久化一致性回归 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_persists_before_creating_actor() {
    // 回归测试：upsert 先写 bot.json 再创建 Actor。
    // 验证：upsert 成功后，repo 里已有数据。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();

    // repo 中必须能读到
    let stored = repo.get(10001).await.unwrap();
    assert!(stored.is_some());
    assert_eq!(stored.unwrap().bot.name, "bot");
}

#[tokio::test]
async fn delete_persists_before_removing_actor() {
    // 回归测试：delete 先删 bot.json 再清理 Actor。
    // 验证：即使 Actor 还在（假设 shutdown 慢），repo 里已无数据。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    manager
        .upsert_bot_config(bot_config(10001, "bot"))
        .await
        .unwrap();
    manager
        .delete_bot_config(&BotId::new("10001"))
        .await
        .unwrap();

    // 持久化已删
    assert_eq!(repo.get(10001).await.unwrap(), None);
    // Actor 也已清除
    assert_eq!(manager.bot_count().await, 0);
}

#[tokio::test]
async fn bootstrap_skipped_bots_are_not_auto_started() {
    // 回归测试：超出上限且标记 auto_start 的 bot 不会被启动。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, repo, _, manager) = make_manager(temp.path());

    // 前 4 个不自动启动，第 5 个标记 auto_start
    for i in 1..=4u64 {
        repo.upsert(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }
    repo.upsert(bot_config_auto_start(10005, "auto-skipped"))
        .await
        .unwrap();

    let result = manager.bootstrap().await.unwrap();

    // 第 5 个被跳过
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0], BotId::new("10005"));

    // 没有 bot 被自动启动（前 4 个不是 auto_start，第 5 个被 skip）
    assert_eq!(result.started.succeeded.len(), 0);
    assert_eq!(manager.bot_count().await, 4);
}

#[tokio::test]
async fn shutdown_all_stops_running_bots_and_clears_actors() {
    // shutdown_all 应该把 active 的 Bot 停掉并清空 actor map。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    prepare_napcat_runtime(temp.path());

    manager
        .upsert_bot_config(bot_config(10001, "bot-1"))
        .await
        .unwrap();
    manager
        .upsert_bot_config(bot_config(10002, "bot-2"))
        .await
        .unwrap();

    manager.start_bot(&BotId::new("10001")).await.unwrap();
    manager.start_bot(&BotId::new("10002")).await.unwrap();
    assert_eq!(manager.active_count().await, 2);

    let result = manager.shutdown_all().await;
    assert_eq!(result.succeeded.len(), 2);
    assert!(result.failed.is_empty());

    // FakeBackend 内部 running set 应被清空。
    assert!(backend.status(BotId::new("10001")).await.unwrap().state == BotActorState::Stopped);
    assert!(backend.status(BotId::new("10002")).await.unwrap().state == BotActorState::Stopped);
    // Actor map 已清空。
    assert_eq!(manager.bot_count().await, 0);
}

#[tokio::test]
async fn process_exit_event_transitions_running_actor_to_crashed() {
    use ncd_runtime::{DomainEvent, DomainEventKind};

    // 当 Running 状态的 Bot 收到非 0 退出，actor 应转为 Crashed。
    let temp = ncd_test_support::TempWorkspace::new().unwrap();

    let store = Arc::new(LocalConfigStore::new(temp.path()));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(temp.path().join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(store.config_dir(), store.config_dir()));
    let backend = Arc::new(FakeBackend::default());
    let event_bus = Arc::new(BroadcastEventBus::default());
    let planner = Arc::new(TestLaunchPlanner);
    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        backend.clone(),
        planner,
        Arc::clone(&event_bus),
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
    );
    prepare_napcat_runtime(temp.path());

    let mut state_sub = event_bus.subscribe(EventFilter::kind(DomainEventKind::BotStateChanged));

    manager
        .upsert_bot_config(bot_config(10010, "bot"))
        .await
        .unwrap();
    manager.start_bot(&BotId::new("10010")).await.unwrap();
    assert_eq!(
        manager
            .get_snapshot(&BotId::new("10010"))
            .await
            .unwrap()
            .state,
        BotActorState::Running
    );

    manager.spawn_runtime_event_listener();
    // 给后台 listener 时间完成 subscribe，否则下面的 publish 会丢。
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 模拟运行时上报：进程崩溃退出，码 1。
    event_bus.publish(DomainEvent::bot_process_exited(
        BotId::new("10010"),
        Some(1),
        None,
    ));

    // 等待 listener 处理事件，最多 2s。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut got_crashed = false;
    while std::time::Instant::now() < deadline {
        if let Some(event) =
            tokio::time::timeout(std::time::Duration::from_millis(200), state_sub.next())
                .await
                .ok()
                .flatten()
            && let DomainEvent::BotStateChanged { snapshot, .. } = event
            && snapshot.bot_id.as_str() == "10010"
            && snapshot.state == BotActorState::Crashed
        {
            got_crashed = true;
            break;
        }
    }
    assert!(got_crashed, "expected actor to transition to Crashed");
}
