//! 远端 Docker NapCat：运行中 WebUI token 是否要重读 / 重发（纯函数）
//!
//! 日志 follow 用 --since 0，会回放容器历史里的旧 token；只按日志切换会来回跳。
//! 所以日志只当触发器，真正发布的值以容器 webui.json 为准，文件读不到才用日志。

use std::time::Duration;

use ncd_domain::NapCatWebuiAuthIssue;
use tokio::time::Instant;

const TOKEN_RECHECK_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(crate) struct TokenRecheck {
    rejected: bool,
    last_attempt: Option<Instant>,
    last_checked_log: Option<String>,
}

impl TokenRecheck {
    pub(crate) fn on_auth_issue(&mut self, issue: Option<NapCatWebuiAuthIssue>) {
        match issue {
            Some(NapCatWebuiAuthIssue::TokenRejected) => self.rejected = true,
            None | Some(NapCatWebuiAuthIssue::TwoFactorRequired) => self.rejected = false,
            // 限流可能盖住 token 错误，成功前仍需核对文件。
            Some(NapCatWebuiAuthIssue::RateLimited | NapCatWebuiAuthIssue::Unreachable) => {}
        }
    }

    pub(crate) fn should_check(&self, published: Option<&str>, latest_log: Option<&str>) -> bool {
        let needed = self.rejected
            || published.is_none()
            || needs_token_recheck(published, latest_log, self.last_checked_log.as_deref());
        needed
            && self
                .last_attempt
                .is_none_or(|at| at.elapsed() >= TOKEN_RECHECK_INTERVAL)
    }

    pub(crate) fn checked(&mut self, from_file: Option<&str>, latest_log: Option<&str>) {
        self.last_attempt = Some(Instant::now());
        // 读失败不能消耗这条触发线索，SSH 恢复后还要重读。
        if from_file.is_some() {
            self.last_checked_log = latest_log.map(str::to_string);
        }
    }

    pub(crate) fn log_fallback_allowed(&self) -> bool {
        !self.rejected
    }
}

/// 日志里出现了与已发布不同、且还没核对过的 token 才值得重读文件
pub(crate) fn needs_token_recheck(
    published: Option<&str>,
    latest_log: Option<&str>,
    last_checked: Option<&str>,
) -> bool {
    let Some(log) = latest_log else {
        return false;
    };
    published != Some(log) && last_checked != Some(log)
}

/// 文件优先，读不到才用日志；与已发布相同返回 None（不重发）
pub(crate) fn token_to_publish(
    published: Option<&str>,
    from_file: Option<&str>,
    from_log: Option<&str>,
) -> Option<String> {
    let candidate = from_file.or(from_log)?;
    (published != Some(candidate)).then(|| candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn rejected_token_is_rechecked_until_auth_recovers() {
        let mut recheck = TokenRecheck::default();
        recheck.on_auth_issue(Some(NapCatWebuiAuthIssue::TokenRejected));
        assert!(recheck.should_check(Some("old"), None));
        recheck.checked(None, None);
        assert!(!recheck.should_check(Some("old"), None));
        tokio::time::advance(TOKEN_RECHECK_INTERVAL).await;
        assert!(recheck.should_check(Some("old"), None));
        assert!(!recheck.log_fallback_allowed());
        recheck.on_auth_issue(Some(NapCatWebuiAuthIssue::RateLimited));
        assert!(recheck.should_check(Some("old"), None));
        recheck.on_auth_issue(None);
        assert!(!recheck.should_check(Some("new"), None));
    }

    #[tokio::test(start_paused = true)]
    async fn unreadable_file_does_not_consume_log_trigger() {
        let mut recheck = TokenRecheck::default();
        recheck.checked(None, Some("new"));
        assert!(!recheck.should_check(Some("old"), Some("new")));
        tokio::time::advance(TOKEN_RECHECK_INTERVAL).await;
        assert!(recheck.should_check(Some("old"), Some("new")));
        recheck.checked(Some("old"), Some("new"));
        tokio::time::advance(TOKEN_RECHECK_INTERVAL).await;
        assert!(!recheck.should_check(Some("old"), Some("new")));
    }

    #[tokio::test(start_paused = true)]
    async fn missing_initial_token_is_retried_without_logs() {
        let mut recheck = TokenRecheck::default();
        assert!(recheck.should_check(None, None));
        recheck.checked(None, None);
        tokio::time::advance(TOKEN_RECHECK_INTERVAL).await;
        assert!(recheck.should_check(None, None));
        assert!(!recheck.should_check(Some("ready"), None));
    }

    #[test]
    fn needs_recheck_only_for_new_unchecked_log_token() {
        assert!(!needs_token_recheck(Some("a"), None, None));
        assert!(!needs_token_recheck(Some("a"), Some("a"), None));
        assert!(needs_token_recheck(Some("a"), Some("b"), None));
        assert!(!needs_token_recheck(Some("a"), Some("b"), Some("b")));
        assert!(needs_token_recheck(None, Some("b"), None));
    }

    #[test]
    fn replayed_old_log_token_does_not_republish() {
        // 文件 == 已发布：日志里的旧 token 不该把它换掉
        assert_eq!(
            token_to_publish(Some("file"), Some("file"), Some("old")),
            None
        );
    }

    #[test]
    fn changed_file_token_republishes() {
        assert_eq!(
            token_to_publish(Some("old"), Some("new"), Some("new")).as_deref(),
            Some("new")
        );
        assert_eq!(
            token_to_publish(None, Some("new"), None).as_deref(),
            Some("new")
        );
    }

    #[test]
    fn unreadable_file_falls_back_to_log_token() {
        assert_eq!(
            token_to_publish(Some("secret-store"), None, Some("from-log")).as_deref(),
            Some("from-log")
        );
        assert_eq!(token_to_publish(Some("same"), None, Some("same")), None);
    }

    #[test]
    fn forced_recheck_without_log_uses_file_only() {
        assert_eq!(
            token_to_publish(Some("rejected"), Some("file"), None).as_deref(),
            Some("file")
        );
        assert_eq!(token_to_publish(Some("rejected"), None, None), None);
    }
}
