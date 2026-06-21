//! SSH 连接配置
//!
//! 默认值:
//! - 默认端口 22
//! - 默认连接超时 30 秒
//! - 默认命令超时 5 分钟(由 HostCommand::timeout 单独控制)
//! - Keepalive 间隔 30 秒

use std::time::Duration;

use crate::remote::credentials::SshCredentials;
use crate::remote::host_key::HostKeyPolicy;

/// SSH 连接参数
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub credentials: SshCredentials,
    pub host_key_policy: HostKeyPolicy,
    pub connect_timeout: Duration,
    pub keepalive_interval: Option<Duration>,
}

impl ConnectionConfig {
    /// 创建一个使用默认超时 / keepalive 的连接配置
    pub fn new(
        host: impl Into<String>,
        port: u16,
        credentials: SshCredentials,
        host_key_policy: HostKeyPolicy,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            credentials,
            host_key_policy,
            connect_timeout: Duration::from_secs(30),
            keepalive_interval: Some(Duration::from_secs(30)),
        }
    }

    pub fn with_connect_timeout(mut self, t: Duration) -> Self {
        self.connect_timeout = t;
        self
    }

    pub fn with_keepalive(mut self, interval: Option<Duration>) -> Self {
        self.keepalive_interval = interval;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_expected_values() {
        let cfg = ConnectionConfig::new(
            "example.com",
            22,
            SshCredentials::password("u", "p"),
            HostKeyPolicy::Insecure,
        );
        assert_eq!(cfg.connect_timeout, Duration::from_secs(30));
        assert_eq!(cfg.keepalive_interval, Some(Duration::from_secs(30)));
    }

    #[test]
    fn custom_overrides_apply() {
        let cfg = ConnectionConfig::new(
            "h",
            2222,
            SshCredentials::password("u", "p"),
            HostKeyPolicy::Insecure,
        )
        .with_connect_timeout(Duration::from_secs(5))
        .with_keepalive(None);
        assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
        assert_eq!(cfg.keepalive_interval, None);
    }
}
