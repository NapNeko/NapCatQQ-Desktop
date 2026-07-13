pub mod napcat;
pub mod remote_native_napcat_session;

// 兼容旧路径: ncd_backend_napcat::remote_native_launch
pub use remote_native_napcat_session::launch as remote_native_launch;

pub use napcat::endpoint_table::NapCatEndpointTable;
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
