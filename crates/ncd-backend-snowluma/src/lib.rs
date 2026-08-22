pub mod remote_snowluma;
pub mod snowluma;

// 兼容旧路径: ncd_backend_snowluma::remote_snowluma_{layout,log,...}
pub use remote_snowluma::layout as remote_snowluma_layout;
pub use remote_snowluma::log as remote_snowluma_log;
pub use remote_snowluma::orchestrator as remote_snowluma_orchestrator;
pub use remote_snowluma::stack as remote_snowluma_stack;
pub use remote_snowluma::tunnel as remote_snowluma_tunnel;

pub use snowluma::{
    AgreementDoc, AgreementsPayload, AuthState, DaemonState, HookProcessInfo, HookProcessStatus,
    LinuxSinglePidProbe, MockProcessTreeProbe, OneBotInstanceInfo, PollerDeps, ProcessTreeProbe,
    ReqwestSnowLumaWebUiClient, ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon,
    SnowLumaDaemonError, SnowLumaLogNoiseFilter, SnowLumaLoginState, SnowLumaRuntimeBackend,
    SnowLumaSession, SnowLumaStatusPoller, SnowLumaWebUiClient, SnowLumaWebUiClientFactory,
    SnowLumaWebUiError, SysinfoProcessTreeProbe, filter_snowluma_console_lines,
    load_or_create_session, load_snowluma_app_config, render_daemon_globals, sanitize_log_line,
};
