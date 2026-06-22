//! SnowLuma 错误类型定义
//!
//! 提供 SnowLumaWebUiError,SnowLumaDaemonError 与
//! From<SnowLumaDaemonError> for BotBackendError 转换
//!
//! 红线:禁止使用 serde_json::Value本文件仅依赖 thiserror,
//! std::path::PathBuf,std::time::Duration,std::collections::BTreeMap,无业务字段透传

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use ncd_traits::runtime_backend::BotBackendError;

/// SnowLuma WebUI HTTP 客户端错误
/// 7 variants,对应 :
/// - Status:HTTP 4xx / 5xx 响应(含 endpoint / status / message)
/// - Timeout:reqwest 超时
/// - Http:网络 / DNS / 连接错误,cause 为字符串化原因(避免暴露 reqwest::Error
///   的内部细节)注: 字段名为 source,但 thiserror v2 会把名为 source
///   的字段自动当作 std::error::Error::source 的 underlying;这里 String 不实现
///   Error,因此沿用语义但改名为 cause 以兼容 thiserror v2Display 仍输出
///   ... source: <text> 与 design 文本一致
/// - Decode:JSON 解码失败 / 字段缺失
/// - NotReady:wait_ready 全部候选 host 都未就绪,附 last_errors(host → 错误描述)
/// - LoginFailed:/api/login 调用失败(含密码错误 / 服务端拒绝)
/// - ServerRejected:load_process / unload_process 服务端返回 success == false
#[derive(Debug, thiserror::Error)]
pub enum SnowLumaWebUiError {
    /// HTTP 4xx / 5xx;status == 0 表示无 HTTP 响应(网络层错误)
    #[error("snowluma webui {endpoint} status={status}: {message}")]
    Status {
        endpoint: String,
        status: u16,
        message: String,
    },
    /// reqwest 超时
    #[error("snowluma webui {endpoint} timeout")]
    Timeout { endpoint: String },
    /// 网络 / DNS / 连接错误cause 为字符串化原因
    #[error("snowluma webui http error on {endpoint}: {cause}")]
    Http { endpoint: String, cause: String },
    /// JSON 解码失败 / 字段缺失
    #[error("snowluma webui {endpoint} decode error: {message}")]
    Decode { endpoint: String, message: String },
    /// wait_ready 30s 全部候选 host 都未就绪
    #[error("snowluma webui not ready after {0:?}; last_errors={1:?}")]
    NotReady(Duration, BTreeMap<String, String>),
    /// /api/login 调用失败(含密码错误 / 服务端拒绝)
    #[error("snowluma webui login failed: {0}")]
    LoginFailed(String),
    /// load_process / unload_process 服务端返回 success == false
    #[error("snowluma webui {endpoint} server rejected: {message}")]
    ServerRejected { endpoint: String, message: String },
}

/// SnowLuma 全局 daemon 错误
/// 9 variants,对应 :
/// - WebUi:透传 SnowLumaWebUiError(#[from] 自动转换)
/// - NodeMissing:node.exe 路径不存在
/// - EntryMissing:SnowLuma 入口脚本路径不存在
/// - Spawn:tokio::process::Command::spawn 失败
/// - Crashed:daemon 当前处于 Crashed 状态(ensure_running 调用方应直接 fail)
/// - Stopping:daemon 当前处于 Stopping 状态
/// - StartTimeout:ensure_running 总超时
/// - Password:密码解析失败(session.json 读写 / 强随机生成)
/// - Io:兜底 IO 错误(路径渲染 / 文件读写)
#[derive(Debug, thiserror::Error)]
pub enum SnowLumaDaemonError {
    #[error(transparent)]
    WebUi(#[from] SnowLumaWebUiError),
    #[error("node.exe not found at {0}")]
    NodeMissing(PathBuf),
    #[error("snowluma entry not found at {0}")]
    EntryMissing(PathBuf),
    #[error("spawn node.exe failed: {0}")]
    Spawn(String),
    #[error("daemon currently crashed: {0}")]
    Crashed(String),
    #[error("daemon currently stopping")]
    Stopping,
    #[error("ensure_running timeout after {0:?}")]
    StartTimeout(Duration),
    #[error("password resolution failed: {0}")]
    Password(String),
    #[error("io error: {0}")]
    Io(String),
}

/// SnowLumaDaemonError → BotBackendError 转换
/// - Crashed / StartTimeout 表达"运行时不可用"语义当前 BotBackendError 尚未引入
///   RuntimeUnavailable variant,按 定义的 fallback 规则映射到 Io 并显式
///   带上 runtime unavailable: 前缀,保留语义;后续若 BotBackendError 扩展该
///   variant,本文件内 match 分支可直接升级,调用点无需改动
/// - 其它 variant 透传到 BotBackendError::Io 并保留原 Display
impl From<SnowLumaDaemonError> for BotBackendError {
    fn from(err: SnowLumaDaemonError) -> Self {
        match err {
            SnowLumaDaemonError::Crashed(msg) => BotBackendError::Io(format!(
                "runtime unavailable: snowluma daemon crashed: {msg}"
            )),
            SnowLumaDaemonError::StartTimeout(d) => BotBackendError::Io(format!(
                "runtime unavailable: snowluma daemon start timeout after {d:?}"
            )),
            other => BotBackendError::Io(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webui_error_status_displays_endpoint_and_status() {
        let err = SnowLumaWebUiError::Status {
            endpoint: "/api/login".into(),
            status: 401,
            message: "unauthorized".into(),
        };
        let rendered = err.to_string();
        assert!(rendered.contains("/api/login"));
        assert!(rendered.contains("401"));
        assert!(rendered.contains("unauthorized"));
    }

    #[test]
    fn webui_error_not_ready_keeps_duration_and_last_errors() {
        let mut last = BTreeMap::new();
        last.insert("localhost".into(), "connect refused".into());
        last.insert("127.0.0.1".into(), "timeout".into());
        let err = SnowLumaWebUiError::NotReady(Duration::from_secs(30), last);
        let rendered = err.to_string();
        assert!(rendered.contains("30"));
        assert!(rendered.contains("localhost"));
        assert!(rendered.contains("127.0.0.1"));
    }

    #[test]
    fn webui_error_server_rejected_displays_message() {
        let err = SnowLumaWebUiError::ServerRejected {
            endpoint: "/api/processes/1234/load".into(),
            message: "process already loaded".into(),
        };
        let rendered = err.to_string();
        assert!(rendered.contains("/api/processes/1234/load"));
        assert!(rendered.contains("process already loaded"));
    }

    #[test]
    fn daemon_error_from_webui_error_uses_transparent() {
        let webui = SnowLumaWebUiError::LoginFailed("bad password".into());
        let webui_text = webui.to_string();
        let daemon: SnowLumaDaemonError = webui.into();
        // #[error(transparent)] 透传 underlying 的 Display
        assert_eq!(daemon.to_string(), webui_text);
        assert!(matches!(daemon, SnowLumaDaemonError::WebUi(_)));
    }

    #[test]
    fn daemon_error_node_missing_displays_path() {
        let err = SnowLumaDaemonError::NodeMissing(PathBuf::from("C:/snowluma/node.exe"));
        let rendered = err.to_string();
        assert!(rendered.contains("node.exe"));
        assert!(rendered.contains("C:/snowluma/node.exe"));
    }

    #[test]
    fn daemon_error_start_timeout_displays_duration() {
        let err = SnowLumaDaemonError::StartTimeout(Duration::from_secs(35));
        assert!(err.to_string().contains("35"));
    }

    #[test]
    fn from_daemon_error_crashed_maps_with_runtime_unavailable_prefix() {
        let err: BotBackendError = SnowLumaDaemonError::Crashed("node exited 1".into()).into();
        match err {
            BotBackendError::Io(msg) => {
                assert!(msg.contains("runtime unavailable"));
                assert!(msg.contains("node exited 1"));
            }
            other => panic!("expected BotBackendError::Io, got {other:?}"),
        }
    }

    #[test]
    fn from_daemon_error_start_timeout_maps_with_runtime_unavailable_prefix() {
        let err: BotBackendError =
            SnowLumaDaemonError::StartTimeout(Duration::from_secs(35)).into();
        match err {
            BotBackendError::Io(msg) => {
                assert!(msg.contains("runtime unavailable"));
                assert!(msg.contains("35"));
            }
            other => panic!("expected BotBackendError::Io, got {other:?}"),
        }
    }

    #[test]
    fn from_daemon_error_other_variants_map_to_io() {
        let cases: Vec<SnowLumaDaemonError> = vec![
            SnowLumaDaemonError::NodeMissing(PathBuf::from("x")),
            SnowLumaDaemonError::EntryMissing(PathBuf::from("y")),
            SnowLumaDaemonError::Spawn("spawn failed".into()),
            SnowLumaDaemonError::Stopping,
            SnowLumaDaemonError::Password("session bad".into()),
            SnowLumaDaemonError::Io("disk full".into()),
            SnowLumaDaemonError::WebUi(SnowLumaWebUiError::Timeout {
                endpoint: "/api/status".into(),
            }),
        ];
        for case in cases {
            let display = case.to_string();
            let mapped: BotBackendError = case.into();
            match mapped {
                BotBackendError::Io(msg) => {
                    // 非 RuntimeUnavailable 路径不带 "runtime unavailable:" 前缀
                    assert!(
                        !msg.starts_with("runtime unavailable:"),
                        "non-crashed/timeout variants must not use runtime-unavailable prefix: {msg}"
                    );
                    assert_eq!(msg, display);
                }
                other => panic!("expected BotBackendError::Io, got {other:?}"),
            }
        }
    }
}
