//! 远端 URL 下载命令拼装（wget / curl）
//!
//! Host::download_url 与 ncd-component 进度桥共用同一套参数 / 超时，
//! 避免两处漂移。进度解析见 download_progress；UI 桥在 component。

use std::time::Duration;

use crate::command::HostCommand;

/// 大包（如 Linux QQ ~180MB）远端直下超时
pub const REMOTE_URL_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);

/// wget 从 URL 写到 dest（进度输出: --progress=dot:mega）
pub fn wget_url_download_command(url: &str, dest_posix: &str) -> HostCommand {
    HostCommand::new("wget")
        .arg("--progress=dot:mega")
        .arg("-O")
        .arg(dest_posix)
        .arg(url)
        .timeout(REMOTE_URL_DOWNLOAD_TIMEOUT)
}

/// curl 从 URL 写到 dest（--progress-bar 到 stderr；-f 遇 HTTP 错失败）
pub fn curl_url_download_command(url: &str, dest_posix: &str) -> HostCommand {
    HostCommand::new("curl")
        .arg("--progress-bar")
        .arg("-fL")
        .arg("-o")
        .arg(dest_posix)
        .arg(url)
        .timeout(REMOTE_URL_DOWNLOAD_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wget_command_sets_long_timeout_and_dot_progress() {
        let cmd = wget_url_download_command("https://example.com/a.deb", "/tmp/a.deb");
        assert_eq!(cmd.program, "wget");
        assert!(cmd.args.iter().any(|a| a == "--progress=dot:mega"));
        assert!(cmd.args.iter().any(|a| a == "/tmp/a.deb"));
        assert_eq!(cmd.timeout, Some(REMOTE_URL_DOWNLOAD_TIMEOUT));
    }

    #[test]
    fn curl_command_fail_on_http_error() {
        let cmd = curl_url_download_command("https://example.com/a.deb", "/tmp/a.deb");
        assert_eq!(cmd.program, "curl");
        assert!(cmd.args.iter().any(|a| a == "-fL"));
        assert_eq!(cmd.timeout, Some(REMOTE_URL_DOWNLOAD_TIMEOUT));
    }
}
