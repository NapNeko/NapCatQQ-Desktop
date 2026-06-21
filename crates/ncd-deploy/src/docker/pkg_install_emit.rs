//! 在 [super::install_progress::InstallProgressEmit] 上跑包管理器流式安装,
//! 与 [ncd_component::pkg_install_stream] 同源解析(parse_pkg_mgr_line),
//! 供 Docker 安装复用组件页的 apt/dnf 进度体验

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_component::{ProgressKind, ProgressLogLevel};
use ncd_host::{
    CommandOutput, Host, HostCommand, StreamSource, host_command_wrap_dpkg_wait_for_apt,
    parse_pkg_mgr_line, truncate_pkg_line,
};

use super::install_progress::InstallProgressEmit;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
const HEARTBEAT_STALE: Duration = Duration::from_secs(22);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

/// 执行 elevated 包管理命令:按行解析 apt/dnf 输出,单调递增 percent(映射到 [floor, cap])
/// 长时间无输出时发心跳,避免 UI 像卡死
pub async fn run_pkg_with_emit(
    host: &dyn Host,
    emit: &InstallProgressEmit,
    cmd: HostCommand,
    step: u32,
    percent_floor: u8,
    percent_cap: u8,
    idle_message: &str,
) -> Result<CommandOutput, ncd_host::HostError> {
    let last_pct = Arc::new(AtomicU8::new(percent_floor));
    let last_activity = Arc::new(AtomicU64::new(now_ms()));

    emit(ProgressKind::StepProgress {
        step,
        percent: percent_floor,
        message: idle_message.to_string(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamMsg>();

    let emit_drain = emit.clone();
    let last_drain = last_pct.clone();
    let drain = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                StreamMsg::Log { level, text } => {
                    emit_drain(ProgressKind::Log {
                        level,
                        message: text,
                    });
                }
                StreamMsg::Progress { percent, message } => {
                    let prev = last_drain.load(Ordering::Relaxed);
                    if percent > prev {
                        last_drain.store(percent, Ordering::Relaxed);
                        emit_drain(ProgressKind::StepProgress {
                            step,
                            percent,
                            message,
                            speed_bps: None,
                            downloaded_bytes: None,
                            total_bytes: None,
                            download_stage: None,
                            docker_layers: None,
                        });
                    }
                }
            }
        }
    });

    let emit_hb = emit.clone();
    let last_hb_pct = last_pct.clone();
    let last_hb_activity = last_activity.clone();
    let heartbeat = tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let last = last_hb_activity.load(Ordering::Relaxed);
            let elapsed_ms = now_ms().saturating_sub(last);
            if elapsed_ms < HEARTBEAT_STALE.as_millis() as u64 {
                continue;
            }
            let pct = last_hb_pct.load(Ordering::Relaxed).max(percent_floor);
            emit_hb(ProgressKind::StepProgress {
                step,
                percent: pct.min(percent_cap),
                message: "仍在执行包管理器，请稍候…".to_string(),
                speed_bps: None,
                downloaded_bytes: None,
                total_bytes: None,
                download_stage: None,
                docker_layers: None,
            });
            last_hb_activity.store(now_ms(), Ordering::Relaxed);
        }
    });

    let tx_cb = tx.clone();
    let last_cb = last_pct.clone();
    let activity_cb = last_activity.clone();
    let floor = percent_floor;
    let cap = percent_cap;

    let cmd = host_command_wrap_dpkg_wait_for_apt(cmd);

    let out = host
        .run_streaming(cmd, {
            Box::new(move |source: StreamSource, line: String| {
                activity_cb.store(now_ms(), Ordering::Relaxed);
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

    heartbeat.abort();
    drop(tx);
    let _ = drain.await;

    let final_pct = last_pct.load(Ordering::Relaxed).max(percent_floor);
    emit(ProgressKind::StepProgress {
        step,
        percent: final_pct.min(percent_cap),
        message: "本阶段已完成".into(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    });

    Ok(out)
}
