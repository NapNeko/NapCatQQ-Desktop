//! 远端 Linux：找出占用项目目录的 systemd 单元，导入时 disable --now 交给桌面端接管。

use std::time::Duration;

use ncd_host::{Host, HostCommand, Locality, shell_single_quote};
use ncd_traits::AppFrameworkError;

/// ExecStart 像在跑框架入口，而不是项目里的 sidecar 脚本。
pub fn exec_looks_like_app(exec: &str, install_dir: &str) -> bool {
    let e = exec.to_ascii_lowercase();
    if !e.contains(&install_dir.to_ascii_lowercase())
        && !e.contains("bot.py")
        && !e.contains("app.mjs")
        && !e.contains("app.js")
        && !e.contains("astrbot")
    {
        return false;
    }
    if e.contains("scripts/") || e.contains("run_admin") || e.contains("run_cg") {
        return false;
    }
    e.contains("bot.py")
        || e.contains("app.mjs")
        || e.contains("app.js")
        || e.contains("node-karin")
        || e.contains("uv run")
        || e.contains("astrbot")
}

pub async fn list_supervisors(
    host: &dyn Host,
    install_dir: &str,
) -> Result<Vec<String>, AppFrameworkError> {
    if host.locality() != Locality::Remote {
        return Ok(Vec::new());
    }
    let dir = shell_single_quote(install_dir);
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

pub async fn disable_now(
    host: &dyn Host,
    units: &[String],
) -> Result<Vec<String>, AppFrameworkError> {
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
pub async fn enable_now(
    host: &dyn Host,
    units: &[String],
) -> Result<Vec<String>, AppFrameworkError> {
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
/// MaiBot 的 Runner 和 Worker 命令行一样（都是 `python bot.py`），取 pid 最小的那个当 Runner：
/// 它先起，而且停的时候要从它开始收整棵树。本机的进程表来自 HashMap，顺序本来就不固定
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
            AppProcessKind::AstrBot => lower.contains("astrbot"),
            AppProcessKind::MaiBot => lower.contains("bot.py"),
            AppProcessKind::Koishi => {
                let slashed = lower.replace('\\', "/");
                slashed.contains("koishi/lib/")
                    || (slashed.contains(".yarn/releases/")
                        && slashed.trim_end().ends_with(" start"))
            }
            // 守护进程 `node app.js daemon` 拉起的子进程是 `node app.js start`，cwd 都是实例目录
            AppProcessKind::Yunzai => lower.contains("app.js") && lower.contains("node"),
            // NeoBot 的控制台入口在实例 .venv 里：Windows `.venv/Scripts/neobot.exe`、
            // 其它 `.venv/bin/neobot`（由 python 解释器执行，命令行走脚本路径）。
            // 调用方已按 cwd == 实例目录过滤，这里再把 .venv 里的入口认出来，
            // 免得把「用户在实例目录里跑的其它命令」也算进来
            AppProcessKind::NeoBot => {
                let slashed = lower.replace('\\', "/");
                slashed.contains(".venv/bin/neobot")
                    || slashed.contains(".venv/scripts/neobot")
            }
        };
        if !hit {
            continue;
        }
        // MaiBot 的 Runner / 云崽的守护进程先起、pid 更小，收树要从它开始；
        // Koishi 同理：yarn → koishi daemon → worker，最外层的 yarn 先起
        if matches!(
            kind,
            AppProcessKind::MaiBot | AppProcessKind::Koishi | AppProcessKind::Yunzai
        ) && best.as_ref().is_some_and(|(p, _)| *p < pid)
        {
            continue;
        }
        let program = if lower.contains("python") {
            "python"
        } else if lower.contains("node") {
            "node"
        } else {
            "python"
        };
        best = Some((pid, program.to_string()));
    }
    best
}

#[derive(Debug, Clone, Copy)]
pub enum AppProcessKind {
    NoneBot2,
    Karin,
    AstrBot,
    MaiBot,
    Koishi,
    Yunzai,
    NeoBot,
}

impl AppProcessKind {
    pub fn from_framework(id: &str) -> Self {
        match id {
            "karin" => Self::Karin,
            "astrbot" => Self::AstrBot,
            "maibot" => Self::MaiBot,
            "koishi" => Self::Koishi,
            "yunzai" => Self::Yunzai,
            "neobot" => Self::NeoBot,
            _ => Self::NoneBot2,
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
    let dir = shell_single_quote(install_dir);
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

    #[test]
    fn pick_astrbot_run() {
        let lines = "\
2201 /opt/astrbot/.venv/bin/python -m astrbot.cli\n\
2202 /home/u/apps/a1/.venv/bin/astrbot run\n";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::AstrBot).unwrap();
        assert_eq!(pid, 2202);
        assert_eq!(prog, "python");
    }

    #[test]
    fn pick_koishi_yarn_launcher_and_skip_desktop_helpers() {
        let lines = r"4410 C:\node\node.exe C:\apps\ko1\node_modules\koishi\lib\worker\index.js
4405 C:\node\node.exe C:\apps\ko1\node_modules\koishi\lib\cli\index.js start
4400 C:\node\node.exe C:\apps\ko1\.yarn\releases\yarn-4.12.0.cjs start
4390 C:\node\node.exe C:\apps\ko1\.yarn\releases\yarn-4.12.0.cjs add koishi-plugin-foo
";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::Koishi).unwrap();
        assert_eq!(pid, 4400, "装插件的 yarn 不算，最外层的 yarn start 先起");
        assert_eq!(prog, "node");
        assert!(matches!(
            AppProcessKind::from_framework("koishi"),
            AppProcessKind::Koishi
        ));
    }

    #[test]
    fn pick_yunzai_daemon_not_child_or_redis() {
        let lines = "\
4411 /home/u/ncd/tools/valkey/bin/valkey-server *:24101\n\
4402 /home/u/node/bin/node /home/u/ncd/apps/yunzai/y1/app.js start\n\
4401 /home/u/node/bin/node /home/u/ncd/apps/yunzai/y1/app.js daemon\n";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::Yunzai).unwrap();
        assert_eq!(pid, 4401, "守护进程先起");
        assert_eq!(prog, "node");
        assert!(matches!(
            AppProcessKind::from_framework("yunzai"),
            AppProcessKind::Yunzai
        ));
    }

    #[test]
    fn pick_maibot_runner_over_worker_in_any_order() {
        let lines = "\
3302 /home/u/apps/m1/.venv/bin/python bot.py\n\
3310 /home/u/apps/m1/.venv/bin/python -m src.plugin_runtime.runner.runner_main\n\
3301 /home/u/apps/m1/.venv/bin/python bot.py\n";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::MaiBot).unwrap();
        assert_eq!(pid, 3301, "Runner 先起，pid 更小");
        assert_eq!(prog, "python");
        assert!(matches!(
            AppProcessKind::from_framework("maibot"),
            AppProcessKind::MaiBot
        ));
    }

    /// 不显式加分支会落到 `_ => Self::NoneBot2`，于是拿 `bot.py` 去匹配 NeoBot 的进程
    #[test]
    fn neobot_kind_and_process_match() {
        assert!(matches!(
            AppProcessKind::from_framework("neobot"),
            AppProcessKind::NeoBot
        ));

        // Linux：解释器执行 .venv/bin/neobot
        let lines = "\
4200 /home/u/apps/n1/.venv/bin/python /home/u/apps/n1/.venv/bin/neobot
\
4201 grep neobot
";
        let (pid, prog) = pick_app_pid(lines, AppProcessKind::NeoBot).unwrap();
        assert_eq!(pid, 4200);
        assert_eq!(prog, "python");

        // Windows：直接跑 .venv/Scripts/neobot.exe；安装目录是 POSIX 写法，命令里是反斜杠
        let win = "\
5200 C:\\apps\\n1\\.venv\\Scripts\\neobot.exe
";
        let (pid, _) = pick_app_pid(win, AppProcessKind::NeoBot).unwrap();
        assert_eq!(pid, 5200);

        // 认不出来的一律不误伤：运行目录里跟 NeoBot 无关的进程
        assert!(
            pick_app_pid("6000 /usr/bin/python other_script.py\n", AppProcessKind::NeoBot)
                .is_none()
        );
    }
}
