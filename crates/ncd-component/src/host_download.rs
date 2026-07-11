//! 远端主机 URL 下载 + 进度桥
//!
//! Host::download_url 不接 ActionCtx（分层：host 不依赖 component）。
//! 这里用与 Host 相同的 wget/curl 命令构造，run_streaming 解析进度；
//! 失败由调用方决定是否本机下载再 upload。

use std::cell::RefCell;
use std::time::Duration;

use ncd_host::{
    CurlProgressParser, DownloadProgress, Host, HostCommand, HostPath, StreamSource,
    WgetProgressParser, curl_url_download_command, wget_url_download_command,
};
use tokio::sync::mpsc;

use crate::context::{ActionCtx, ProgressKind};
use crate::error::ActionError;

enum StreamMsg {
    Progress {
        percent: u8,
        message: String,
        speed_bps: Option<u64>,
        downloaded_bytes: Option<u64>,
    },
}

enum ProgressParser {
    Wget(WgetProgressParser),
    Curl(CurlProgressParser),
}

impl ProgressParser {
    fn parse_line(&mut self, line: &str) -> Option<DownloadProgress> {
        match self {
            Self::Wget(p) => p.parse_line(line),
            Self::Curl(p) => p.parse_line(line),
        }
    }
}

/// 在 host 上从 url 下载到 dest，并向 ctx 上报 step 的 StepProgress。
///
/// `expected_sha256` 为 Some 时在远端跑 sha256sum 校验，失败删半截文件。
/// 取消：与下载竞态，命中后 best-effort 删 dest（SSH channel 可能稍后才停）。
pub async fn download_url_to_host_with_progress(
    host: &dyn Host,
    url: &str,
    dest: &HostPath,
    ctx: &ActionCtx,
    step: u32,
    expected_sha256: Option<&str>,
) -> Result<(), ActionError> {
    if ctx.is_cancelled() {
        return Err(ActionError::Cancelled);
    }

    let has_wget = host.command_exists("wget").await;
    let has_curl = !has_wget && host.command_exists("curl").await;
    if !has_wget && !has_curl {
        return Err(ActionError::Host(ncd_host::HostError::Unsupported {
            operation: "download_url (wget/curl not found)".into(),
        }));
    }

    let dest_str = dest.as_posix().to_string();
    let program = if has_wget { "wget" } else { "curl" };
    let cmd = if has_wget {
        wget_url_download_command(url, &dest_str)
    } else {
        curl_url_download_command(url, &dest_str)
    };

    ctx.emit(ProgressKind::StepProgress {
        step,
        percent: 0,
        message: format!("远端下载 {program}"),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: Some("remote".into()),
        docker_layers: None,
    })
    .await;

    let (tx, mut rx) = mpsc::unbounded_channel::<StreamMsg>();
    let drain_ctx = ctx.clone();
    let drain_task = tokio::spawn(async move {
        let mut last_pct: u8 = 0;
        while let Some(msg) = rx.recv().await {
            match msg {
                StreamMsg::Progress {
                    percent,
                    message,
                    speed_bps,
                    downloaded_bytes,
                } => {
                    if percent >= last_pct {
                        last_pct = percent;
                        drain_ctx
                            .emit(ProgressKind::StepProgress {
                                step,
                                percent,
                                message,
                                speed_bps,
                                downloaded_bytes,
                                total_bytes: None,
                                download_stage: Some("remote".into()),
                                docker_layers: None,
                            })
                            .await;
                    }
                }
            }
        }
    });

    // 回调串行：RefCell 足够，不必 Mutex
    let parser = RefCell::new(if has_wget {
        ProgressParser::Wget(WgetProgressParser::new())
    } else {
        ProgressParser::Curl(CurlProgressParser::new())
    });
    let tx_cb = tx.clone();
    let cancel_for_cb = ctx.cancel_token();
    let download_fut = host.run_streaming(
        cmd,
        Box::new(move |_source: StreamSource, line: String| {
            if cancel_for_cb.is_cancelled() {
                return;
            }
            let progress = parser.borrow_mut().parse_line(&line);
            if let Some(p) = progress {
                let msg = if p.speed_bps > 0 {
                    format!("远端下载 {}% ({})", p.percent, fmt_bps(p.speed_bps))
                } else {
                    format!("远端下载 {}%", p.percent)
                };
                let _ = tx_cb.send(StreamMsg::Progress {
                    percent: p.percent.min(99),
                    message: msg,
                    speed_bps: (p.speed_bps > 0).then_some(p.speed_bps),
                    downloaded_bytes: (p.downloaded_bytes > 0).then_some(p.downloaded_bytes),
                });
            }
        }),
    );

    let cancel_watch = ctx.cancel_token();
    let out = tokio::select! {
        biased;
        _ = cancel_watch.cancelled() => {
            drop(tx);
            let _ = drain_task.await;
            let _ = host.remove_file(dest).await;
            return Err(ActionError::Cancelled);
        }
        result = download_fut => result,
    };

    drop(tx);
    let _ = drain_task.await;

    let out = match out {
        Ok(o) => o,
        Err(e) => {
            let _ = host.remove_file(dest).await;
            return Err(ActionError::Host(e));
        }
    };

    if !out.success() {
        let _ = host.remove_file(dest).await;
        return Err(ActionError::DownloadFailed {
            url: redact_url_for_error(url),
            reason: format!(
                "remote {program} exit={:?}: {}",
                out.exit_code,
                out.stderr.lines().take(5).collect::<Vec<_>>().join(" ")
            ),
        });
    }

    if let Some(expected) = expected_sha256 {
        if let Err(e) = verify_remote_sha256(host, dest, expected).await {
            let _ = host.remove_file(dest).await;
            return Err(e);
        }
    }

    if ctx.is_cancelled() {
        let _ = host.remove_file(dest).await;
        return Err(ActionError::Cancelled);
    }

    ctx.emit(ProgressKind::StepProgress {
        step,
        percent: 100,
        message: "远端下载完成".into(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: Some("remote".into()),
        docker_layers: None,
    })
    .await;

    Ok(())
}

async fn verify_remote_sha256(
    host: &dyn Host,
    path: &HostPath,
    expected: &str,
) -> Result<(), ActionError> {
    let path_str = path.as_posix();
    // sha256sum 输出: "<hex>  <path>"；部分系统只有 openssl
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(format!(
            "if command -v sha256sum >/dev/null 2>&1; then sha256sum {p}; \
             elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 {p} | awk '{{print $NF}}'; \
             else echo 'no-sha256-tool' >&2; exit 127; fi",
            p = shell_single_quote(path_str)
        ))
        .timeout(Duration::from_secs(120));
    let out = host.run_to_string(cmd).await.map_err(ActionError::Host)?;
    if !out.success() {
        return Err(ActionError::install_step(
            "remote_sha256",
            format!(
                "远端校验工具不可用或失败: exit={:?} stderr={}",
                out.exit_code,
                out.stderr.trim()
            ),
        ));
    }
    let actual = out
        .stdout
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let expected_l = expected.trim().to_ascii_lowercase();
    if actual.is_empty() || actual != expected_l {
        return Err(ActionError::ChecksumMismatch {
            expected: expected_l,
            actual: if actual.is_empty() {
                "(empty)".into()
            } else {
                actual
            },
        });
    }
    Ok(())
}

/// 错误日志用：去掉 query（UrlSign 的 sign/t 等）
fn redact_url_for_error(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => format!("{base}?[redacted]"),
        None => url.to_string(),
    }
}

fn shell_single_quote(s: &str) -> String {
    // POSIX: 'foo'\''bar'
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn fmt_bps(bps: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bps >= MB {
        format!("{:.1} MB/s", bps as f64 / MB as f64)
    } else if bps >= KB {
        format!("{:.1} KB/s", bps as f64 / KB as f64)
    } else {
        format!("{bps} B/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_url_strips_query() {
        assert_eq!(
            redact_url_for_error("https://qqdl.gtimg.cn/a.deb?sign=abc&t=1"),
            "https://qqdl.gtimg.cn/a.deb?[redacted]"
        );
        assert_eq!(
            redact_url_for_error("https://dldir1.qq.com/a.deb"),
            "https://dldir1.qq.com/a.deb"
        );
    }

    #[test]
    fn shell_single_quote_escapes_quote() {
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }
}
