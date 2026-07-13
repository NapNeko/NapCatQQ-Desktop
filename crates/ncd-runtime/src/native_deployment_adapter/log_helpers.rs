//! 远端日志 tail 辅助

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;

/// 远端日志尾部：SSH `tail -n`，禁止 SFTP 整文件（crash dump 可至百 MB）。
pub(crate) async fn remote_tail_log_raw_lines(
    host: &dyn Host,
    path: &str,
    max_raw_lines: usize,
) -> Result<Vec<String>, BotBackendError> {
    if max_raw_lines == 0 {
        return Ok(Vec::new());
    }
    let quoted = path.replace('\'', "'\"'\"'");
    let cmd = HostCommand::new("sh").arg("-c").arg(format!(
        "if [ -f '{quoted}' ]; then tail -n {max_raw_lines} -- '{quoted}'; else exit 0; fi"
    ));
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(out.stdout.lines().map(|s| s.to_string()).collect())
}
