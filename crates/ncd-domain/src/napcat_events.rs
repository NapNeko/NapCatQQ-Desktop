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

/// Desktop 换不到 NapCat WebUI 凭据的原因（徽章用，None 表示已恢复）
/// - TokenRejected: 上游回 token is invalid / Unauthorized，token 与容器不一致
/// - RateLimited: 上游 loginRate 限流（按来源 IP 计数）
/// - TwoFactorRequired: WebUI 开了两步验证，Desktop 无法代登
/// - Unreachable: 连续超时 / 连接失败 / 响应不可解析
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum NapCatWebuiAuthIssue {
    TokenRejected,
    RateLimited,
    TwoFactorRequired,
    Unreachable,
}

/// NapCat WebUI 会话快照，供前端重建时恢复端点和鉴权状态。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct NapcatWebuiBindingDto {
    pub bot_id: String,
    pub port: u16,
    pub token: String,
    pub online: Option<bool>,
    pub auth_issue: Option<NapCatWebuiAuthIssue>,
}
