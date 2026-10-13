// 区分远端指标文件缺失与连接失败，断网不能推断探针未注入。
use ncd_domain::{BotId, BotRuntimeMetrics, ProbeHealth};
use ncd_host::HostError;

pub fn remote_metrics_read_failure(
    bot_id: BotId,
    error: &HostError,
    now: u64,
) -> BotRuntimeMetrics {
    let missing = matches!(error, HostError::PathNotFound { .. })
        || matches!(error, HostError::Io(err) if err.kind() == std::io::ErrorKind::NotFound);
    let mut metrics = BotRuntimeMetrics::unavailable(bot_id);
    metrics.collected_at_ms = now;
    metrics.probe = if missing {
        ProbeHealth::NotInjected
    } else {
        ProbeHealth::Error
    };
    metrics.probe_error = Some(if missing {
        "远端尚未生成指标快照".into()
    } else if error.is_disconnect() || matches!(error, HostError::Timeout { .. }) {
        "远端指标暂时不可用，等待连接恢复".into()
    } else {
        format!("读取远端指标失败：{error}")
    });
    metrics
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_host::HostPath;

    #[test]
    fn disconnect_timeout_and_permissions_do_not_request_bot_restart() {
        for error in [
            HostError::remote_disconnected("ssh session poisoned for sftp"),
            HostError::Timeout { operation: "sftp" },
            HostError::PermissionDenied {
                path: HostPath::from_posix("/metrics"),
                operation: "read",
            },
            HostError::Io(std::io::Error::from(std::io::ErrorKind::ConnectionReset)),
        ] {
            let metrics = remote_metrics_read_failure(BotId::new("123"), &error, 42);
            assert_eq!(metrics.probe, ProbeHealth::Error);
            assert_eq!(metrics.collected_at_ms, 42);
            assert!(!metrics.probe_error.unwrap().contains("重启"));
        }
    }

    #[test]
    fn only_missing_files_are_not_injected() {
        for error in [
            HostError::PathNotFound {
                path: HostPath::from_posix("/metrics"),
            },
            HostError::Io(std::io::Error::from(std::io::ErrorKind::NotFound)),
        ] {
            assert_eq!(
                remote_metrics_read_failure(BotId::new("123"), &error, 42).probe,
                ProbeHealth::NotInjected
            );
        }
    }
}
