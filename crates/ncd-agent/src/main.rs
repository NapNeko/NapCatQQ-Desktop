//! Windows 远端常驻入口；bridge stdout 专用于 NDJSON。
#[cfg(windows)]
mod bot;
#[cfg(windows)]
mod install;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod runner;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod store;
#[cfg(windows)]
mod supervisor;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod wire;

fn main() {
    if let Err(error) = run() {
        eprintln!("ncd-agent: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|s| s == "--version") {
        println!("ncd-agent {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    #[cfg(windows)]
    {
        let mode = arguments.first().map(String::as_str).unwrap_or("probe");
        let option = |name: &str| {
            arguments
                .windows(2)
                .find(|p| p[0] == name)
                .map(|p| p[1].clone())
        };
        let root = option("--root")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into()),
                )
                .join("NapCatQQ Desktop")
                .join("remote")
            });
        match mode {
            "probe" => wire::write_frame(&mut std::io::stdout().lock(), &native::probe(&root)?)?,
            "bridge" => ipc::bridge(&root, option("--user").as_deref())?,
            "service" => service::dispatch(root)?,
            "session-worker" => supervisor::session_worker(root)?,
            "runner" => runner::run(root, option("--instance").ok_or("--instance required")?, option("--generation").ok_or("--generation required")?)?,
            "install" => install::install(&root, option("--owner-sid").as_deref())?,
            "uninstall" => install::uninstall(&root)?,
            "register-session" => install::register_session(&root)?,
            _ => return Err("usage: ncd-agent probe|bridge|install|uninstall|service|session-worker|register-session --root <path>".into()),
        }
        Ok(())
    }
    #[cfg(not(windows))]
    Err("ncd-agent requires Windows".into())
}
