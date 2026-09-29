//! 远端文件/日志读取小工具

use ncd_host::{Host, HostPath};
use ncd_traits::runtime_backend::BotBackendError;

pub(crate) async fn host_file_nonempty(host: &dyn Host, path: &str) -> bool {
    match host.read_file(&HostPath::from_posix(path)).await {
        Ok(b) => !b.is_empty(),
        Err(_) => false,
    }
}

pub(crate) async fn read_remote_file_trimmed(
    host: &dyn Host,
    path: &str,
) -> Result<String, BotBackendError> {
    let bytes = host
        .read_file(&HostPath::from_posix(path))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(String::from_utf8_lossy(&bytes).trim().to_string())
}

pub(crate) async fn read_remote_log_tail(
    host: &dyn Host,
    path: &str,
    max_lines: usize,
) -> Result<String, BotBackendError> {
    let bytes = match host.read_file(&HostPath::from_posix(path)).await {
        Ok(b) => b,
        Err(_) => return Ok(String::new()),
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let start = if lines.len() > max_lines {
        lines.len() - max_lines
    } else {
        0
    };
    Ok(lines[start..].join("\n"))
}

/// 只拉末尾原始行（不整文件 SFTP）。噪声多时再本地 filter。
pub(crate) async fn read_remote_log_tail_lines(
    host: &dyn Host,
    path: &str,
    max_raw_lines: usize,
) -> Result<Vec<String>, BotBackendError> {
    ncd_host::remote_tail_lines(host, path, max_raw_lines, None)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))
}
