//! 换凭据失败的分类与退避（纯函数）
//!
//! 上游 /api/auth/login 按来源 IP 限流（Docker 默认 loginRate: 3 / 60s），Desktop 隧道、
//! 浏览器里的 WebUI、宿主机上的 ncd-watch 在容器看来是同一个 IP。固定 5s 重试会把
//! 用户手动登录的额度也吃掉，所以明确拒绝要退到 60s 窗口之外。

use std::time::Duration;

use crate::napcat::webui_client::NapCatWebUiError;
use ncd_domain::napcat_events::NapCatWebuiAuthIssue;

/// 比上游 60s 限流窗口多一点，保证下一次落在新窗口
const RATE_WINDOW_RETRY: Duration = Duration::from_secs(65);
const TRANSIENT_BASE: Duration = Duration::from_secs(5);
const TRANSIENT_CAP: Duration = Duration::from_secs(60);
const REJECTED_CAP: Duration = Duration::from_secs(300);
/// 暂时性失败连续这么多次（约 75s）才报不可达，避免容器慢启动时徽章闪一下
const UNREACHABLE_AFTER: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthFailureKind {
    Rejected,
    RateLimited,
    TwoFactor,
    Transient,
}

impl AuthFailureKind {
    pub(crate) fn from_error(err: &NapCatWebUiError) -> Self {
        match err {
            NapCatWebUiError::Unauthorized(_)
            | NapCatWebUiError::Rejected(_)
            | NapCatWebUiError::BusinessCode { .. } => Self::Rejected,
            NapCatWebUiError::RateLimited => Self::RateLimited,
            NapCatWebUiError::TwoFactorRequired => Self::TwoFactor,
            NapCatWebUiError::Status(_)
            | NapCatWebUiError::Throttled
            | NapCatWebUiError::Timeout
            | NapCatWebUiError::Http(_)
            | NapCatWebUiError::Decode(_)
            | NapCatWebUiError::NotLogin => Self::Transient,
        }
    }
}

/// streak 从 1 开始，按同类失败连续次数算
pub(crate) fn retry_delay(kind: AuthFailureKind, streak: u32) -> Duration {
    let exp = streak.saturating_sub(1).min(16);
    match kind {
        AuthFailureKind::Transient => TRANSIENT_BASE
            .saturating_mul(1u32 << exp)
            .min(TRANSIENT_CAP),
        AuthFailureKind::RateLimited => RATE_WINDOW_RETRY,
        AuthFailureKind::Rejected | AuthFailureKind::TwoFactor => RATE_WINDOW_RETRY
            .saturating_mul(1u32 << exp)
            .min(REJECTED_CAP),
    }
}

pub(crate) fn issue_for(kind: AuthFailureKind, streak: u32) -> Option<NapCatWebuiAuthIssue> {
    match kind {
        AuthFailureKind::Rejected => Some(NapCatWebuiAuthIssue::TokenRejected),
        AuthFailureKind::RateLimited => Some(NapCatWebuiAuthIssue::RateLimited),
        AuthFailureKind::TwoFactor => Some(NapCatWebuiAuthIssue::TwoFactorRequired),
        AuthFailureKind::Transient => {
            (streak >= UNREACHABLE_AFTER).then_some(NapCatWebuiAuthIssue::Unreachable)
        }
    }
}

/// elapsed=None 表示从未尝试过，首次必过
pub(crate) fn refresh_allowed(
    elapsed: Option<Duration>,
    throttle: Duration,
    backoff: Duration,
) -> bool {
    match elapsed {
        None => true,
        Some(elapsed) => elapsed >= throttle.max(backoff),
    }
}

/// 写进 warn 的中文原因，用户看日志能直接知道往哪查
pub(crate) fn hint(kind: AuthFailureKind) -> &'static str {
    match kind {
        AuthFailureKind::Rejected => "WebUI token 被拒绝，和 NapCat 实际使用的 token 不一致",
        AuthFailureKind::RateLimited => "WebUI 登录被限流（loginRate），等下一个窗口再试",
        AuthFailureKind::TwoFactor => "WebUI 开启了两步验证，Desktop 无法自动登录",
        AuthFailureKind::Transient => "WebUI 暂时不可达或响应异常",
    }
}
