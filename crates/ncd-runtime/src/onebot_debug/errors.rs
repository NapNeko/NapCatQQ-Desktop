//! 把三类客户端（NapCat 调试接口、SnowLuma 调试接口、OneBot HTTP / WS）的失败
//! 折算成界面上的 [`DebugError`]，并顺带给出这次失败说明了通道处于什么状态。
//!
//! 状态只在失败真能说明通道本身的情况时才给（连不上、鉴权没过、上游太老……）；
//! 超时、上游回了 500 这类只说明「这一次」不行，不去改通道状态。

use std::time::Duration;

use ncd_backend_napcat::NapCatDebugError;
use ncd_backend_snowluma::SnowLumaDebugError;
use ncd_domain::onebot_debug::{DebugChannelStatus, DebugError};
use ncd_onebot::client::ClientError;
use serde_json::{Value, json};

/// NapCat 对未知动作回的 WebUI 业务错误文案（`不支持的 API: foo`）
const NAPCAT_UNKNOWN_ACTION: &str = "不支持的 API";
/// NapCat 调用时唯一一条说明「通道本身不行」的业务错误，其余都是动作自己抛的
const NAPCAT_ADAPTER_MISSING: &str = "调试适配器不存在";
/// 按 OneBot 11 的约定，未知动作的 retcode
const RETCODE_UNKNOWN_ACTION: i64 = 1404;
/// NapCat 的 OneBot 层把处理函数抛出的异常回成 retcode 200，走真实通道时看到的也是它
const RETCODE_HANDLER_ERROR: i64 = 200;

/// 一次失败：给前端的错误，以及可选的通道状态更新
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub error: DebugError,
    pub status: Option<DebugChannelStatus>,
}

impl Failure {
    pub fn new(error: DebugError) -> Self {
        Self {
            error,
            status: None,
        }
    }

    fn with_status(error: DebugError, status: DebugChannelStatus) -> Self {
        Self {
            error,
            status: Some(status),
        }
    }

    /// 通道本身连不上：错误和状态用同一段原因
    pub fn unreachable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::with_status(
            DebugError::ChannelUnavailable {
                reason: reason.clone(),
            },
            DebugChannelStatus::Unreachable { reason },
        )
    }

    /// 通道注定连不上（配置决定的），状态也记成「不支持」
    pub fn unsupported(reason: String) -> Self {
        Self::with_status(
            DebugError::ChannelUnavailable {
                reason: reason.clone(),
            },
            DebugChannelStatus::Unsupported { reason },
        )
    }

    pub fn too_old() -> Self {
        Self::with_status(
            DebugError::UpstreamTooOld,
            DebugChannelStatus::UpstreamTooOld,
        )
    }

    fn auth(status: u16) -> Self {
        Self::with_status(
            DebugError::AuthFailed { status },
            DebugChannelStatus::AuthFailed { status },
        )
    }

    fn timeout(timeout: Duration) -> Self {
        Self::new(DebugError::Timeout {
            ms: duration_ms(timeout),
        })
    }

    fn transport(message: impl Into<String>) -> Self {
        Self::new(DebugError::Transport {
            message: message.into(),
        })
    }

    /// 「测试连通」要给每种结果都落一个状态：没有显式状态的失败按错误本身折算
    pub fn probe_status(&self) -> DebugChannelStatus {
        if let Some(status) = &self.status {
            return status.clone();
        }
        match &self.error {
            DebugError::BotNotRunning => DebugChannelStatus::BotNotRunning,
            DebugError::NotLoggedIn => DebugChannelStatus::NotLoggedIn,
            DebugError::UpstreamTooOld => DebugChannelStatus::UpstreamTooOld,
            DebugError::AuthFailed { status } => DebugChannelStatus::AuthFailed { status: *status },
            DebugError::Timeout { ms } => DebugChannelStatus::Unreachable {
                reason: format!("{} 秒内没有响应", ms.div_ceil(1000)),
            },
            DebugError::ChannelUnavailable { reason } => DebugChannelStatus::Unreachable {
                reason: reason.clone(),
            },
            DebugError::Transport { message } => DebugChannelStatus::Unreachable {
                reason: message.clone(),
            },
            other => DebugChannelStatus::Unreachable {
                reason: other.kind_str().to_owned(),
            },
        }
    }
}

/// 错误给人看的一句话：事件流里调用记录的 `error`、接收器停下的原因都用它。
/// 与前端 mock 的 `errorText` 同一套说法，真机和演示模式看到的一样。
/// Tauri 命令层把 `Result<_, DebugError>` 折成 `Err(String)` 时也用它
pub fn error_text(error: &DebugError) -> String {
    match error {
        DebugError::BotNotFound => "Bot 不存在".to_owned(),
        DebugError::BotNotRunning => "Bot 没有在运行".to_owned(),
        DebugError::NotLoggedIn => "QQ 还没登录".to_owned(),
        DebugError::ChannelUnavailable { reason } => format!("通道不可用：{reason}"),
        DebugError::UpstreamTooOld => "上游版本太老，没有调试接口".to_owned(),
        DebugError::AuthFailed { status } => format!("鉴权失败（HTTP {status}）"),
        DebugError::Timeout { ms } => format!("等待超过 {ms} 毫秒"),
        DebugError::Cancelled => "已取消".to_owned(),
        DebugError::Transport { message }
        | DebugError::InvalidParams { message }
        | DebugError::Internal { message } => message.clone(),
        DebugError::FeatureDisabled => "调试台已关闭".to_owned(),
    }
}

pub(crate) fn duration_ms(timeout: Duration) -> u32 {
    u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX)
}

/// NapCat 调试接口在动作未知或处理函数抛异常时，回的不是 OB11 回包而是 WebUI 业务错误
/// （比如参数缺字段时的 `Cannot read properties of undefined`）。这其实是「拿到了回答」：
/// 折成一个失败回包（未知动作 1404，其余 200），界面照常展示，而不是当成通道故障。
/// 只有「调试适配器不存在」真说明通道不行，留给 [`from_napcat`]
pub(crate) fn napcat_call_failure_reply(err: &NapCatDebugError) -> Option<Value> {
    let NapCatDebugError::Business { message, .. } = err else {
        return None;
    };
    if message.contains(NAPCAT_ADAPTER_MISSING) {
        return None;
    }
    let retcode = if message.contains(NAPCAT_UNKNOWN_ACTION) {
        RETCODE_UNKNOWN_ACTION
    } else {
        RETCODE_HANDLER_ERROR
    };
    Some(json!({
        "status": "failed",
        "retcode": retcode,
        "data": null,
        "message": message,
        "wording": message,
    }))
}

pub(crate) fn from_napcat(err: NapCatDebugError, timeout: Duration) -> Failure {
    match err {
        NapCatDebugError::TooOld => Failure::too_old(),
        NapCatDebugError::Unauthorized => Failure::auth(401),
        // 两步验证是上游设置决定的，重试没用，也不是「连不上」
        e @ NapCatDebugError::TwoFactorRequired => {
            let reason = e.to_string();
            Failure::with_status(
                DebugError::ChannelUnavailable {
                    reason: reason.clone(),
                },
                DebugChannelStatus::Unsupported { reason },
            )
        }
        NapCatDebugError::Timeout => Failure::timeout(timeout),
        // Http 包含建连失败：WebUI 根本没连上
        e @ NapCatDebugError::Http(_) => {
            let message = e.to_string();
            Failure::with_status(
                DebugError::Transport {
                    message: message.clone(),
                },
                DebugChannelStatus::Unreachable { reason: message },
            )
        }
        e @ (NapCatDebugError::Status(_) | NapCatDebugError::Decode(_)) => {
            Failure::transport(e.to_string())
        }
        NapCatDebugError::Business { message, .. } => {
            Failure::new(DebugError::ChannelUnavailable { reason: message })
        }
    }
}

pub(crate) fn from_snowluma(err: SnowLumaDebugError, timeout: Duration) -> Failure {
    match err {
        SnowLumaDebugError::TooOld => Failure::too_old(),
        SnowLumaDebugError::Unauthorized => Failure::auth(401),
        e @ (SnowLumaDebugError::LoginBlocked(_) | SnowLumaDebugError::NeedsConsent) => {
            Failure::unreachable(e.to_string())
        }
        SnowLumaDebugError::NotOnline => {
            Failure::with_status(DebugError::NotLoggedIn, DebugChannelStatus::NotLoggedIn)
        }
        SnowLumaDebugError::Timeout => Failure::timeout(timeout),
        e @ SnowLumaDebugError::Http(_) => {
            let message = e.to_string();
            Failure::with_status(
                DebugError::Transport {
                    message: message.clone(),
                },
                DebugChannelStatus::Unreachable { reason: message },
            )
        }
        // 没带「需要同意协议」标记的 403：多半是来源 IP 被挡，按鉴权失败提示
        SnowLumaDebugError::Status { status: 403, .. } => Failure::auth(403),
        e @ (SnowLumaDebugError::Status { .. } | SnowLumaDebugError::Decode(_)) => {
            Failure::transport(e.to_string())
        }
    }
}

pub(crate) fn from_client(err: ClientError, timeout: Duration) -> Failure {
    match err {
        ClientError::Timeout => Failure::timeout(timeout),
        ClientError::Unauthorized(status) => Failure::auth(status),
        e @ ClientError::Connect(_) => Failure::unreachable(e.to_string()),
        // 连接在等回包时断了：下次调用会重连，先把状态标成连不上
        e @ ClientError::Closed => {
            let message = e.to_string();
            Failure::with_status(
                DebugError::Transport {
                    message: message.clone(),
                },
                DebugChannelStatus::Unreachable { reason: message },
            )
        }
        // 请求还没写出去连接就关了（多半是接收器停下时关掉了这条共用连接）：说明不了通道有问题，
        // 不改状态、不拆隧道。调用方会先换一条连接重发一次
        e @ ClientError::NotSent => Failure::transport(e.to_string()),
        ClientError::Status { status, body } => {
            let body = body.trim();
            if body.is_empty() {
                Failure::transport(format!("HTTP {status}"))
            } else {
                let preview: String = body.chars().take(200).collect();
                Failure::transport(format!("HTTP {status}：{preview}"))
            }
        }
        e @ ClientError::Protocol(_) => Failure::transport(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_millis(1500);

    #[test]
    fn napcat_errors_map_to_error_and_status() {
        assert_eq!(from_napcat(NapCatDebugError::TooOld, T), Failure::too_old());
        assert_eq!(
            from_napcat(NapCatDebugError::Unauthorized, T),
            Failure::auth(401)
        );
        let f = from_napcat(NapCatDebugError::TwoFactorRequired, T);
        assert!(matches!(f.error, DebugError::ChannelUnavailable { .. }));
        assert!(matches!(
            f.status,
            Some(DebugChannelStatus::Unsupported { .. })
        ));
        assert_eq!(
            from_napcat(NapCatDebugError::Timeout, T).error,
            DebugError::Timeout { ms: 1500 }
        );
        let f = from_napcat(NapCatDebugError::Http("refused".into()), T);
        assert!(matches!(f.error, DebugError::Transport { .. }));
        assert!(matches!(
            f.status,
            Some(DebugChannelStatus::Unreachable { .. })
        ));
        for e in [
            NapCatDebugError::Status(500),
            NapCatDebugError::Decode("x".into()),
        ] {
            let f = from_napcat(e, T);
            assert!(matches!(f.error, DebugError::Transport { .. }));
            assert_eq!(f.status, None);
        }
        let f = from_napcat(
            NapCatDebugError::Business {
                code: -1,
                message: "适配器不存在".into(),
            },
            T,
        );
        assert_eq!(
            f.error,
            DebugError::ChannelUnavailable {
                reason: "适配器不存在".into()
            }
        );
    }

    #[test]
    fn napcat_unknown_action_becomes_a_1404_reply() {
        let err = NapCatDebugError::Business {
            code: -1,
            message: "不支持的 API: foo".into(),
        };
        let reply = napcat_call_failure_reply(&err).unwrap();
        assert_eq!(reply["retcode"], 1404);
        assert_eq!(reply["status"], "failed");
        assert_eq!(reply["message"], "不支持的 API: foo");
        assert!(napcat_call_failure_reply(&NapCatDebugError::TooOld).is_none());
    }

    #[test]
    fn napcat_handler_exception_becomes_a_200_reply_but_missing_adapter_stays_a_channel_error() {
        let thrown = NapCatDebugError::Business {
            code: -1,
            message: "Cannot read properties of undefined (reading 'type')".into(),
        };
        let reply = napcat_call_failure_reply(&thrown).unwrap();
        assert_eq!(reply["retcode"], 200);
        assert_eq!(reply["status"], "failed");
        assert_eq!(
            reply["wording"],
            "Cannot read properties of undefined (reading 'type')"
        );

        let missing = NapCatDebugError::Business {
            code: -1,
            message: "调试适配器不存在".into(),
        };
        assert!(napcat_call_failure_reply(&missing).is_none());
        assert!(matches!(
            from_napcat(missing, T).error,
            DebugError::ChannelUnavailable { .. }
        ));
    }

    #[test]
    fn snowluma_errors_map_to_error_and_status() {
        assert_eq!(
            from_snowluma(SnowLumaDebugError::TooOld, T),
            Failure::too_old()
        );
        assert_eq!(
            from_snowluma(SnowLumaDebugError::NotOnline, T),
            Failure::with_status(DebugError::NotLoggedIn, DebugChannelStatus::NotLoggedIn)
        );
        for e in [
            SnowLumaDebugError::LoginBlocked("密码错误".into()),
            SnowLumaDebugError::NeedsConsent,
        ] {
            let f = from_snowluma(e, T);
            assert!(matches!(f.error, DebugError::ChannelUnavailable { .. }));
        }
        assert_eq!(
            from_snowluma(
                SnowLumaDebugError::Status {
                    status: 403,
                    message: String::new()
                },
                T
            ),
            Failure::auth(403)
        );
        let f = from_snowluma(
            SnowLumaDebugError::Status {
                status: 500,
                message: "boom".into(),
            },
            T,
        );
        assert!(matches!(f.error, DebugError::Transport { .. }));
        assert_eq!(f.status, None);
    }

    #[test]
    fn client_errors_map_to_error_and_status() {
        assert_eq!(
            from_client(ClientError::Timeout, T).error,
            DebugError::Timeout { ms: 1500 }
        );
        assert_eq!(
            from_client(ClientError::Unauthorized(403), T),
            Failure::auth(403)
        );
        let f = from_client(ClientError::Connect("refused".into()), T);
        assert!(matches!(f.error, DebugError::ChannelUnavailable { .. }));
        assert!(matches!(
            f.status,
            Some(DebugChannelStatus::Unreachable { .. })
        ));
        let f = from_client(
            ClientError::Status {
                status: 500,
                body: "  oops ".into(),
            },
            T,
        );
        assert_eq!(
            f.error,
            DebugError::Transport {
                message: "HTTP 500：oops".into()
            }
        );
        assert_eq!(f.status, None);
        let f = from_client(ClientError::Closed, T);
        assert!(matches!(f.error, DebugError::Transport { .. }));
        assert!(f.status.is_some());
        let f = from_client(ClientError::NotSent, T);
        assert!(matches!(f.error, DebugError::Transport { .. }));
        assert_eq!(f.status, None, "没发出去的请求不说明通道有问题");
    }

    #[test]
    fn error_text_reads_like_the_frontend() {
        assert_eq!(
            error_text(&DebugError::AuthFailed { status: 401 }),
            "鉴权失败（HTTP 401）"
        );
        assert_eq!(
            error_text(&DebugError::ChannelUnavailable {
                reason: "连不上".into()
            }),
            "通道不可用：连不上"
        );
        assert_eq!(
            error_text(&DebugError::Transport {
                message: "HTTP 500".into()
            }),
            "HTTP 500"
        );
    }

    #[test]
    fn probe_status_falls_back_to_the_error() {
        assert_eq!(
            Failure::new(DebugError::Timeout { ms: 10_000 }).probe_status(),
            DebugChannelStatus::Unreachable {
                reason: "10 秒内没有响应".into()
            }
        );
        assert_eq!(
            Failure::auth(401).probe_status(),
            DebugChannelStatus::AuthFailed { status: 401 }
        );
        assert_eq!(
            Failure::transport("HTTP 500").probe_status(),
            DebugChannelStatus::Unreachable {
                reason: "HTTP 500".into()
            }
        );
    }
}
