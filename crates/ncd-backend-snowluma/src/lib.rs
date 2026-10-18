pub mod docker_login;
pub mod qr_guard;
pub mod remote_snowluma;
pub mod snowluma;

// 兼容旧路径: ncd_backend_snowluma::remote_snowluma_{layout,log,...}
pub use remote_snowluma::layout as remote_snowluma_layout;
pub use remote_snowluma::log as remote_snowluma_log;
pub use remote_snowluma::tunnel as remote_snowluma_tunnel;

pub use snowluma::{
    AgreementDoc, AgreementsPayload, HookProcessInfo, LinuxSinglePidProbe, PollerDeps,
    ReqwestSnowLumaWebUiClient, ReqwestSnowLumaWebUiClientFactory, SlStreamFrame, SnowLumaDaemon,
    SnowLumaDebugClient, SnowLumaDebugError, SnowLumaLogNoiseFilter, SnowLumaRuntimeBackend,
    SnowLumaStatusPoller, SnowLumaWebUiClient, filter_snowluma_console_lines, parse_stream_frame,
};
