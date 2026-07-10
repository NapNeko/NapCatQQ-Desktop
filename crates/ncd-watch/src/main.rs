//! ncd-watch CLI

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use ncd_watch::config::{NotifyConfig, WatchConfig, WatchPaths};
use ncd_watch::probe::HostProber;
use ncd_watch::run::{run_loop, run_once};
use ncd_watch::edge::EdgeTracker;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "ncd-watch", version, about = "Remote host watcher for NapCatQQ Desktop offline webhooks")]
struct Cli {
    /// 安装根目录(默认 $HOME/ncd-watch)
    #[arg(long, global = true, env = "NCD_WATCH_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 常驻探活(默认子命令语义)
    Run,
    /// 只跑一轮探活并打印结果(不发 Webhook 除非有边沿且 Desktop 离线)
    Once,
    /// 打印默认路径与示例配置
    PrintDefaults,
    /// 写出默认 watch.json / 示例 notify.json(不覆盖已有文件,除非 --force)
    Init {
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_tracing();

    let paths = match &cli.root {
        Some(r) => WatchPaths::from_root(r),
        None => WatchPaths::default_home(),
    };

    match cli.cmd {
        Commands::PrintDefaults => {
            println!("root={}", paths.root.display());
            println!("watch.json={}", paths.watch_json.display());
            println!("notify.json={}", paths.notify_json.display());
            println!("desktop_present={}", paths.desktop_present.display());
            println!(
                "\n# watch.json\n{}",
                serde_json::to_string_pretty(&WatchConfig::default()).unwrap_or_default()
            );
            println!(
                "\n# notify.json example\n{}",
                serde_json::to_string_pretty(&NotifyConfig::example()).unwrap_or_default()
            );
        }
        Commands::Init { force } => {
            if let Err(e) = init_layout(&paths, force) {
                eprintln!("init failed: {e}");
                std::process::exit(1);
            }
            println!("initialized under {}", paths.root.display());
        }
        Commands::Once => {
            let watch = WatchConfig::load_or_default(&paths.watch_json)
                .unwrap_or_default()
                .clamp();
            let notify = NotifyConfig::load_or_default(&paths.notify_json).unwrap_or_default();
            let mut edges = EdgeTracker::load(&paths.edge_state, watch.debounce_secs);
            let out = run_once(&paths, &watch, &notify, &HostProber, &mut edges).await;
            println!(
                "probed={} fired={} debounced={} desktop_present_skip={} errors={:?}",
                out.probed, out.fired, out.debounced, out.skipped_desktop_present, out.webhook_errors
            );
        }
        Commands::Run => {
            tracing::info!(root = %paths.root.display(), "ncd-watch starting");
            let (tx, rx) = tokio::sync::watch::channel(false);
            let prober: Arc<dyn ncd_watch::Prober> = Arc::new(HostProber);
            let paths2 = paths.clone();
            let handle = tokio::spawn(async move {
                run_loop(paths2, prober, rx).await;
            });

            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).expect("sigterm");
                let mut sigint = signal(SignalKind::interrupt()).expect("sigint");
                let mut sighup = signal(SignalKind::hangup()).expect("sighup");
                loop {
                    tokio::select! {
                        _ = sigterm.recv() => {
                            tracing::info!("SIGTERM");
                            let _ = tx.send(true);
                            break;
                        }
                        _ = sigint.recv() => {
                            tracing::info!("SIGINT");
                            let _ = tx.send(true);
                            break;
                        }
                        _ = sighup.recv() => {
                            // 配置每轮循环重读;HUP 仅记日志
                            tracing::info!("SIGHUP (config reloaded on next tick)");
                        }
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
                let _ = tx.send(true);
            }

            let _ = handle.await;
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

fn init_layout(paths: &WatchPaths, force: bool) -> Result<(), String> {
    for d in [&paths.bin_dir, &paths.config_dir, &paths.state_dir, &paths.log_dir] {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    write_if_missing(&paths.watch_json, &WatchConfig::default(), force)?;
    write_if_missing(&paths.notify_json, &NotifyConfig::example(), force)?;
    Ok(())
}

fn write_if_missing<T: serde::Serialize>(path: &std::path::Path, value: &T, force: bool) -> Result<(), String> {
    if path.is_file() && !force {
        return Ok(());
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| e.to_string())
}
