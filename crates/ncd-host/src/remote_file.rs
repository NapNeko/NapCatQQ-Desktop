//! 按路径在远端 shell 里取文件大小、从某个偏移读一段、读末尾几行。
//!
//! 跟随日志和取日志尾巴都走这几条:整文件走 SFTP 可能碰上上百 MB 的崩溃转储,
//! 让远端先截好再传。脚本文本各处原来抄了好几份,改引号或截断上限只改这里。

use std::time::Duration;

use crate::command::HostCommand;
use crate::error::HostError;
use crate::host::Host;
use crate::shell::shell_single_quote;

/// 跟随日志时一次最多读这么多字节;刚接上时也只从末尾这么多开始读
pub const LOG_FOLLOW_CHUNK_BYTES: u64 = 512 * 1024;

fn sh(script: String) -> HostCommand {
    HostCommand::new("sh").arg("-c").arg(script)
}

/// 文件字节数;文件不存在算 0,命令跑不起来或输出不是数字是 None
pub async fn remote_file_size(host: &dyn Host, path: &str) -> Option<u64> {
    let quoted = shell_single_quote(path);
    let out = host
        .run_to_string(sh(format!(
            "if [ -f {quoted} ]; then wc -c < {quoted}; else echo 0; fi"
        )))
        .await
        .ok()?;
    out.stdout.trim().parse().ok()
}

/// 从 offset 字节处起最多读 max 字节;文件不存在读到空
pub async fn remote_read_from(
    host: &dyn Host,
    path: &str,
    offset: u64,
    max: u64,
) -> Option<Vec<u8>> {
    let quoted = shell_single_quote(path);
    let start = offset.saturating_add(1);
    let out = host
        .run_to_string(sh(format!(
            "if [ -f {quoted} ]; then tail -c +{start} -- {quoted} | head -c {max}; fi"
        )))
        .await
        .ok()?;
    Some(out.stdout.into_bytes())
}

/// 末尾 n 行原样返回;文件不存在是空列表
///
/// timeout 为 None 时用 Host 默认上限;开页就拉的调用方给短一点的上限,SSH 卡住时页面不跟着干等
pub async fn remote_tail_lines(
    host: &dyn Host,
    path: &str,
    n: usize,
    timeout: Option<Duration>,
) -> Result<Vec<String>, HostError> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let quoted = shell_single_quote(path);
    let mut cmd = sh(format!(
        "if [ -f {quoted} ]; then tail -n {n} -- {quoted}; else exit 0; fi"
    ));
    if let Some(timeout) = timeout {
        cmd = cmd.timeout(timeout);
    }
    let out = host.run_to_string(cmd).await?;
    Ok(out.stdout.lines().map(str::to_string).collect())
}
