//! 离线告警投递:Webhook / Email / OneBot + fan-out

mod composite;
mod email;
mod onebot;
mod webhook;

pub use composite::{
    CompositeOfflineNotifier, DesktopToastSink, NoopOneBotEndpointResolver, OneBotEndpointResolver,
    SwappableOneBotEndpointResolver,
};
pub use email::{send_offline_email, send_test_email};
pub use onebot::{
    LocalHttpServerCandidate, MessengerResolveSkip, OneBotMessenger, resolve_local_onebot_messenger,
};
pub use webhook::{send_offline_webhook, send_test_webhook};
