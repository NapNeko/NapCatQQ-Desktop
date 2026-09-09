//! 远端 Linux：找出占用项目目录的 systemd 单元，导入时 disable --now 交给桌面端接管。

use std::time::Duration;

use ncd_host::{Host, HostCommand, Locality};
use ncd_traits::AppFrameworkError;

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// ExecStart 像在跑框架入口，而不是项目里的 sidecar 脚本。
pub fn exec_looks_like_app(exec: &str, install_dir: &str) -> bool {
    let e = exec.to_ascii_lowercase();
    if !e.contains(&install_dir.to_ascii_lowercase()) && !e.contains("bot.py") && !e.contains("app.mjs")
    {
        return false;
    }
    if e.contains("scripts/") || e.contains("run_admin") || e.contains("run_cg") {
        return false;
    }
    e.contains("bot.py")
        || e.contains("app.mjs")
        || e.contains("node-karin")
        || e.contains("uv run")
}

pub async fn list_supervisors(
    host: &dyn Host,
    install_dir: &str,
) -> Result<Vec<String>, AppFrameworkError> {
    if host.locality() != Locality::Remote {
        return Ok(Vec::new());
    }
    let dir = shell_quote(install_dir);
    let script = format!(
        "dir={dir}\n\
         for f in /etc/systemd/system/*.service; do\n\
           [ -f \"$f\" ] || continue\n\
           grep -Fq \"$dir\" \"$f\" 2>/dev/null || continue\n\
           exec=$(grep -E '^ExecStart=' \"$f\" | head -n1 | sed 's/^ExecStart=//')\n\
           wd=$(grep -E '^WorkingDirectory=' \"$f\" | head -n1 | sed 's/^WorkingDirectory=//')\n\
           unit=$(basename \"$f\" .service)\n\
           echo \"$unit|$wd|$exec\"\n\
         done"
    );
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(20)),
        )
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(units_from_probe(&out.stdout, install_dir))
}

pub fn units_from_probe(stdout: &str, install_dir: &str) -> Vec<String> {
    let mut units = Vec::new();
    for raw in stdout.lines() {
        let mut parts = raw.splitn(3, '|');
        let Some(unit) = parts.next() else {
            continue;
        };
        let wd = parts.next().unwrap_or("");
        let exec = parts.next().unwrap_or("");
        let _ = wd;
        if exec_looks_like_app(exec, install_dir) && !unit.is_empty() {
            units.push(unit.trim().to_string());
        }
    }
    units
}

pub async fn disable_now(host: &dyn Host, units: &[String]) -> Result<Vec<String>, AppFrameworkError> {
    if units.is_empty() || host.locality() != Locality::Remote {
        return Ok(Vec::new());
    }
    let mut done = Vec::new();
    for unit in units {
        match systemctl(host, &["disable", "--now", unit]).await {
            Ok(()) => done.push(unit.clone()),
            Err(e) => {
                if let Err(rb) = enable_now(host, &done).await {
                    tracing::error!(error = %rb, "re-enable systemd after partial disable");
                }
                return Err(e);
            }
        }
    }
    Ok(done)
}

/// 把接管时停掉的单元还给系统。失败不吞，调用方据此决定要不要注销实例。
pub async fn enable_now(host: &dyn Host, units: &[String]) -> Result<Vec<String>, AppFrameworkError> {
    if units.is_empty() || host.locality() != Locality::Remote {
        return Ok(Vec::new());
    }
    let mut done = Vec::new();
    for unit in units {
        systemctl(host, &["enable", "--now", unit]).await?;
        done.push(unit.clone());
    }
    Ok(done)
}

async fn systemctl(host: &dyn Host, args: &[&str]) -> Result<(), AppFrameworkError> {
    let unit = args.last().copied().unwrap_or_default();
    let mut cmd = HostCommand::new("systemctl");
    for arg in args {
        cmd = cmd.arg(*arg);
    }
    let out = host
        .run_to_string(cmd.timeout(Duration::from_secs(30)))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if out.success() {
        Ok(())
    } else {
        Err(AppFrameworkError::Runtime(format!(
            "systemctl {} {unit} 失败: {}",
            args.first().unwrap_or(&""),
            out.stderr.trim()
        )))
    }
}

/// 在项目目录里找框架主进程（跳过 admin / cg 脚本）。
pub fn pick_app_pid(lines: &str, kind: AppProcessKind) -> Option<(u32, String)> {
    let mut best: Option<(u32, String)> = None;
    for raw in lines.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let Ok(pid) = parts.next()?.parse::<u32>() else {
            continue;
        };
        let cmd = parts.next().unwrap_or("");
        let lower = cmd.to_ascii_lowercase();
        if lower.contains("scripts/") || lower.contains("run_admin") || lower.contains("run_cg") {
            continue;
        }
        let hit = match kind {
            AppProcessKind::NoneBot2 => lower.contains("bot.py"),
            AppProcessKind::Karin => {
                lower.contains("app.mjs") || lower.contains("node-karin") || lower.contains("karin")
            }
        };
        if hit {
            let program = if lower.contains("python") {
                "python"
            } else if lower.contains("node") {
                "node"
            } else {
                "python"
            };
            best = Some((pid, program.to_string()));
        }
    }
    best
}

#[derive(Debug, Clone, Copy)]
pub enum AppProcessKind {
    NoneBot2,
    Karin,
}

impl AppProcessKind {
    pub fn from_framework(id: &str) -> Self {
        if id == "karin" {
            Self::Karin
        } else {
            Self::NoneBot2
        }
    }
}

pub async fn list_cwd_processes(
    host: &dyn Host,
    install_dir: &str,
) -> Result<String, AppFrameworkError> {
    if host.locality() != Locality::Remote {
        return Ok(String::new());
    }
    let dir = shell_quote(install_dir);
    let script = format!(
        "dir={dir}\n\
         for d in /proc/[0-9]*; do\n\
           pid=${{d#/proc/}}\n\
           cwd=$(readlink \"$d/cwd\" 2>/dev/null) || continue\n\
           [ \"$cwd\" = \"$dir\" ] || continue\n\
           cmd=$(tr '\\0' ' ' < \"$d/cmdline\" 2>/dev/null)\n\
           echo \"$pid $cmd\"\n\
         done"
    );
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(15)),
        )
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_sidecar_exec() {
        let dir = "/root/game-qqbot/bot-xiuxian";
        assert!(exec_looks_like_app(
            "/root/.local/bin/uv run python bot.py",
            dir
        ));
        assert!(!exec_looks_like_app(
            "/root/game-qqbot/bot-xiuxian/.venv/bin/python scripts/run_admin.py",
            dir
        ));
        assert!(!exec_looks_like_app(
            "/root/game-qqbot/bot-xiuxian/.venv/bin/python scripts/run_cg_http.py",
            dir
        ));
    }

    #[test]
    fn units_from_xiuxian_probe() {
        let dir = "/root/game-qqbot/bot-xiuxian";
        let out = "\
bot-xiuxian|/root/game-qqbot/bot-xiuxian|/root/.local/bin/uv run python bot.py\n\
bot-xiuxian-admin|/root/game-qqbot/bot-xiuxian|/root/game-qqbot/bot-xiuxian/.venv/bin/python scripts/run_admin.py\n\
xiuxian-cg-http|/root/game-qqbot/bot-xiuxian|/root/game-qqbot/bot-xiuxian/.venv/bin/python scripts/run_cg_http.py\n";
        assert_eq!(units_from_probe(out, dir), vec!["bot-xiuxian".to_string()]);
    }

    #[test]
    fn pick_bot_py_not_admin() {
        let lines = "\
1004 /root/game-qqbot/bot-xiuxian/.venv/bin/python scripts/run_cg_http.py\n\
659109 /root/game-qqbot/bot-xiuxian/.venv/bin/python3 bot.py\n\
659110 /root/game-qqbot/bot-xiuxian/.venv/bin/python3 scripts/run_admin.py\n";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::NoneBot2).unwrap();
        assert_eq!(pid, 659109);
        assert_eq!(prog, "python");
    }
}
