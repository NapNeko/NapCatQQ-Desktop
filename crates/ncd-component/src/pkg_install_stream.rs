//! 在主机上执行包管理器命令并流式解析 stdout/stderr,上报 [ActionCtx] 进度

use std::sync::atomic::{AtomicU8, Ordering};

use ncd_host::{
    CommandOutput, Host, HostCommand, StreamSource, host_command_wrap_dpkg_wait_for_apt,
    output_indicates_dpkg_lock_hold, parse_pkg_mgr_line, truncate_pkg_line,
};
use tokio::sync::mpsc;

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::error::ActionError;

enum StreamMsg {
    Log {
        level: ProgressLogLevel,
        text: String,
    },
    Progress {
        percent: u8,
        message: String,
    },
}

/// 执行 cmd(通常为 sh -c "apt-get …" / dnf …),按行解析 apt/dnf 输出并
/// 向 ctx 发送 Log + StepProgress(百分比单调递增,封顶 percent_cap)
pub async fn run_pkg_command_with_progress(
    host: &dyn Host,
    ctx: &mut ActionCtx,
    cmd: HostCommand,
    step: u32,
    percent_floor: u8,
    percent_cap: u8,
    idle_message: &str,
) -> Result<CommandOutput, ActionError> {
    if ctx.is_cancelled() {
        return Err(ActionError::Cancelled);
    }

    let last_pct = std::sync::Arc::new(AtomicU8::new(percent_floor));
    let (tx, mut rx) = mpsc::unbounded_channel::<StreamMsg>();

    let drain_ctx = ctx.clone();
    let drain_last = last_pct.clone();
    let drain_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                StreamMsg::Log { level, text } => {
                    drain_ctx.log(level, text).await;
                }
                StreamMsg::Progress { percent, message } => {
                    let prev = drain_last.load(Ordering::Relaxed);
                    if percent > prev {
                        drain_last.store(percent, Ordering::Relaxed);
                        drain_ctx
                            .emit(ProgressKind::StepProgress {
                                step,
                                percent,
                                message,
                                speed_bps: None,
                                downloaded_bytes: None,
                                total_bytes: None,
                                download_stage: None,
                                docker_layers: None,
                            })
                            .await;
                    }
                }
            }
        }
    });

    ctx.emit(ProgressKind::StepProgress {
        step,
        percent: percent_floor,
        message: idle_message.to_string(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    })
    .await;

    let tx_cb = tx.clone();
    let last_cb = last_pct.clone();
    let floor = percent_floor;
    let cap = percent_cap;

    let cmd = host_command_wrap_dpkg_wait_for_apt(cmd);

    let out = host
        .run_streaming(cmd, {
            Box::new(move |source: StreamSource, line: String| {
                let t = line.trim();
                if t.is_empty() {
                    return;
                }

                let parsed = parse_pkg_mgr_line(t);
                let (summary, suggest, level) = if let Some(p) = parsed {
                    let lvl = if matches!(p.phase, ncd_host::PkgPhase::Error) {
                        ProgressLogLevel::Warn
                    } else {
                        ProgressLogLevel::Info
                    };
                    (p.summary, p.suggest_percent, lvl)
                } else if source == StreamSource::Stderr
                    && (t.contains("WARNING") || t.contains("warning"))
                {
                    (truncate_pkg_line(t, 200), None, ProgressLogLevel::Warn)
                } else {
                    return;
                };

                let _ = tx_cb.send(StreamMsg::Log {
                    level,
                    text: summary.clone(),
                });

                if let Some(mut pct) = suggest {
                    pct = pct.clamp(floor, cap);
                    let prev = last_cb.load(Ordering::Relaxed);
                    if pct > prev {
                        let _ = tx_cb.send(StreamMsg::Progress {
                            percent: pct,
                            message: summary,
                        });
                    }
                }
            })
        })
        .await?;

    drop(tx);
    let _ = drain_task.await;

    if ctx.is_cancelled() {
        return Err(ActionError::Cancelled);
    }

    if !out.success() {
        let detail = if !out.stderr.trim().is_empty() {
            out.stderr.trim().to_string()
        } else {
            out.stdout.trim().to_string()
        };
        ctx.log(ProgressLogLevel::Error, truncate_pkg_line(&detail, 240))
            .await;
        let hint = if output_indicates_dpkg_lock_hold(&out) {
            "apt 被系统占用（常见 unattended-upgr）。请稍后在组件页重试，或远端执行: sudo systemctl stop unattended-upgrades"
        } else {
            ""
        };
        let msg = if hint.is_empty() {
            truncate_pkg_line(&detail, 400)
        } else {
            format!("{}\n{}", truncate_pkg_line(&detail, 280), hint)
        };
        return Err(ActionError::install_step("pkg_install", msg));
    }

    let final_pct = last_pct.load(Ordering::Relaxed).max(percent_floor);
    ctx.emit(ProgressKind::StepProgress {
        step,
        percent: final_pct.min(percent_cap),
        message: "包管理器命令已完成".into(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    })
    .await;

    Ok(out)
}
