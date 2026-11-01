//! SnowLuma：backend re-export + Desktop 侧协议同意 / UI 态。

pub(crate) mod agreements;
pub(crate) mod consent_files;
mod docker_agreements;
pub mod ui_state;

pub use ncd_backend_snowluma::snowluma::*;
pub use ui_state::{SnowLumaUiBotSnapshot, SnowLumaUiSnapshot, SnowLumaUiStateTable};
