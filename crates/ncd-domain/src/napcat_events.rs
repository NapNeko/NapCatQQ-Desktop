// NapCat 事件相关数据类型
//
// 纯 serde + ts-rs,零运行时依赖。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// NapCat WebUI 登录失效的原因
/// - Kicked: 在线状态下账号被踢下线
/// - LoggedOut: 用户主动登出或会话过期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum NapCatLoginInvalidationReason {
    Kicked,
    LoggedOut,
}
