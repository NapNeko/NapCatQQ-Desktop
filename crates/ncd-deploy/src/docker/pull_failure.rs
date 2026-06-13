//! 将 docker pull / Host 错误归类为用户可读的失败原因（超时 vs 连不上 vs 其它）。

use ncd_host::HostError;

use super::cli::DockerCliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullFailureKind {
    CommandTimeout,
    SshOrChannelTimeout,
    ConnectionLost,
    MirrorUnreachable,
    AuthOrDenied,
    DockerDaemon,
    Other,
}

impl PullFailureKind {
    pub fn user_title(self) -> &'static str {
        match self {
            Self::CommandTimeout => "拉取超时",
            Self::SshOrChannelTimeout => "SSH/通道超时",
            Self::ConnectionLost => "连接中断",
            Self::MirrorUnreachable => "无法连接镜像源",
            Self::AuthOrDenied => "认证或权限被拒绝",
            Self::DockerDaemon => "Docker 守护进程异常",
            Self::Other => "拉取失败",
        }
    }
}

fn classify_text_blob(blob: &str) -> Option<PullFailureKind> {
    let b = blob.to_ascii_lowercase();
    if b.contains("i/o timeout")
        || b.contains("context deadline exceeded")
        || b.contains("timed out")
        || b.contains("timeout")
    {
        return Some(PullFailureKind::MirrorUnreachable);
    }
    if b.contains("connection refused")
        || b.contains("no route to host")
        || b.contains("network is unreachable")
        || b.contains("could not resolve")
        || b.contains("name or service not known")
        || b.contains("tls handshake")
        || b.contains("dial tcp")
    {
        return Some(PullFailureKind::MirrorUnreachable);
    }
    if b.contains("unauthorized")
        || b.contains("authentication required")
        || b.contains("access denied")
        || b.contains("permission denied")
    {
        return Some(PullFailureKind::AuthOrDenied);
    }
    if b.contains("cannot connect to the docker daemon")
        || b.contains("is the docker daemon running")
    {
        return Some(PullFailureKind::DockerDaemon);
    }
    None
}

pub fn classify_pull_failure(err: &DockerCliError) -> (PullFailureKind, String) {
    match err {
        DockerCliError::Host(HostError::Timeout { operation }) => {
            let kind = if operation.contains("streaming") {
                PullFailureKind::SshOrChannelTimeout
            } else {
                PullFailureKind::CommandTimeout
            };
            (
                kind,
                format!(
                    "{}：命令在限定时间内未结束。大镜像或网络较慢时可重试；若长期无层进度，可能是镜像站连不上。",
                    kind.user_title()
                ),
            )
        }
        DockerCliError::Host(HostError::RemoteDisconnected { reason }) => (
            PullFailureKind::ConnectionLost,
            format!(
                "{}：{}。请检查 SSH 与远端网络后重试。",
                PullFailureKind::ConnectionLost.user_title(),
                reason.trim()
            ),
        ),
        DockerCliError::Host(e) => {
            let detail = e.to_string();
            let kind = classify_text_blob(&detail).unwrap_or(PullFailureKind::Other);
            (kind, format!("{}：{}", kind.user_title(), detail))
        }
        DockerCliError::CommandFailed { stderr, .. } => {
            let kind = classify_text_blob(stderr)
                .or_else(|| classify_text_blob(&err.to_string()))
                .unwrap_or(PullFailureKind::Other);
            let tail = stderr.trim();
            let detail = if tail.is_empty() {
                err.to_string()
            } else if tail.len() > 240 {
                format!("{}…", &tail[..240])
            } else {
                tail.to_string()
            };
            (
                kind,
                format!("{}：{}", kind.user_title(), detail),
            )
        }
        DockerCliError::RuntimeUnavailable { .. } | DockerCliError::ParseFailed(_) => (
            PullFailureKind::Other,
            err.to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::DockerStatus;

    #[test]
    fn classifies_timeout_as_ssh_when_streaming() {
        let err = DockerCliError::Host(HostError::Timeout {
            operation: "remote_run_streaming",
        });
        let (k, _) = classify_pull_failure(&err);
        assert_eq!(k, PullFailureKind::SshOrChannelTimeout);
    }

    #[test]
    fn classifies_connection_refused_in_stderr() {
        let err = DockerCliError::CommandFailed {
            command: "docker pull".into(),
            exit_code: Some(1),
            stderr: "Error response from daemon: Get https://registry-1.docker.io/v2/: dial tcp: connection refused".into(),
        };
        let (k, _) = classify_pull_failure(&err);
        assert_eq!(k, PullFailureKind::MirrorUnreachable);
    }

    #[test]
    fn runtime_unavailable_is_other() {
        let err = DockerCliError::RuntimeUnavailable {
            status: DockerStatus::absent(),
        };
        let (k, _) = classify_pull_failure(&err);
        assert_eq!(k, PullFailureKind::Other);
    }
}