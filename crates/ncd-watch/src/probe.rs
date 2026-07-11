//! 进程 / Docker 探活(登录态探活另层,见 LoginStatus)

use std::path::Path;
use std::process::Stdio;

use crate::config::NotifyBotTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    Online,
    Offline,
    /// 无法判断(docker 未装、权限等);不触发边沿
    Unknown,
}

/// 登录层;Unknown 表示未探或探失败,不触发登录边沿
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginStatus {
    #[default]
    Unknown,
    LoggedIn,
    LoggedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Docker,
    PidFile,
    ProcessMatch,
    /// 未配置任何探活手段
    Unconfigured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub bot_id: String,
    pub kind: ProbeKind,
    /// 进程 / 容器层
    pub status: ProbeStatus,
    /// 登录层(无信号时保持 Unknown)
    pub login: LoginStatus,
    pub detail: String,
}

/// 可注入的探活后端(测试用 mock)
pub trait Prober: Send + Sync {
    fn probe_bot(&self, bot: &NotifyBotTarget) -> ProbeResult;
}

/// 默认:本机 docker CLI + pid 文件 + pgrep
#[derive(Debug, Default, Clone, Copy)]
pub struct HostProber;

impl Prober for HostProber {
    fn probe_bot(&self, bot: &NotifyBotTarget) -> ProbeResult {
        if bot.is_docker() {
            return probe_docker(bot);
        }
        if let Some(pid_file) = bot
            .pid_file
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return probe_pid_file(bot, Path::new(pid_file));
        }
        if let Some(pat) = bot
            .process_match
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return probe_process_match(bot, pat);
        }
        ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::Unconfigured,
            status: ProbeStatus::Unknown,
            login: LoginStatus::Unknown,
            detail: "no pid_file / process_match / docker target".into(),
        }
    }
}

fn process_only(
    bot: &NotifyBotTarget,
    kind: ProbeKind,
    status: ProbeStatus,
    detail: String,
) -> ProbeResult {
    ProbeResult {
        bot_id: bot.bot_id.clone(),
        kind,
        status,
        login: LoginStatus::Unknown,
        detail,
    }
}

fn probe_docker(bot: &NotifyBotTarget) -> ProbeResult {
    let name = bot.resolved_container_name();
    let output = std::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", &name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout)
                .trim()
                .to_ascii_lowercase();
            let online = text == "true";
            process_only(
                bot,
                ProbeKind::Docker,
                if online {
                    ProbeStatus::Online
                } else {
                    ProbeStatus::Offline
                },
                format!("container {name} running={text}"),
            )
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let missing = err.contains("No such object") || err.contains("no such object");
            process_only(
                bot,
                ProbeKind::Docker,
                if missing {
                    ProbeStatus::Offline
                } else {
                    ProbeStatus::Unknown
                },
                format!("docker inspect {name}: {}", err.trim()),
            )
        }
        Err(e) => process_only(
            bot,
            ProbeKind::Docker,
            ProbeStatus::Unknown,
            format!("docker not available: {e}"),
        ),
    }
}

fn probe_pid_file(bot: &NotifyBotTarget, path: &Path) -> ProbeResult {
    if !path.is_file() {
        return process_only(
            bot,
            ProbeKind::PidFile,
            ProbeStatus::Offline,
            format!("pid file missing: {}", path.display()),
        );
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            return process_only(
                bot,
                ProbeKind::PidFile,
                ProbeStatus::Unknown,
                format!("read pid file: {e}"),
            );
        }
    };
    let pid: i32 = match text.trim().parse() {
        Ok(p) if p > 0 => p,
        _ => {
            return process_only(
                bot,
                ProbeKind::PidFile,
                ProbeStatus::Offline,
                "pid file empty or invalid".into(),
            );
        }
    };
    let online = pid_alive(pid);
    process_only(
        bot,
        ProbeKind::PidFile,
        if online {
            ProbeStatus::Online
        } else {
            ProbeStatus::Offline
        },
        format!("pid={pid} alive={online}"),
    )
}

fn probe_process_match(bot: &NotifyBotTarget, pattern: &str) -> ProbeResult {
    // processMatch 常为 `-q <qq>$`；若不加 `--`，procps pgrep 会把 `-q` 当选项并失败，
    // 探活永远 Offline，边沿/Webhook/Email 全哑火。
    let output = std::process::Command::new("pgrep")
        .args(["-f", "--", pattern])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(out) if out.status.success() => process_only(
            bot,
            ProbeKind::ProcessMatch,
            ProbeStatus::Online,
            format!("pgrep -f -- {pattern} matched"),
        ),
        Ok(_) => process_only(
            bot,
            ProbeKind::ProcessMatch,
            ProbeStatus::Offline,
            format!("pgrep -f -- {pattern} no match"),
        ),
        Err(e) => process_only(
            bot,
            ProbeKind::ProcessMatch,
            ProbeStatus::Unknown,
            format!("pgrep unavailable: {e}"),
        ),
    }
}

fn pid_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 测试用:按 bot_id 固定返回进程/登录层
#[derive(Debug, Default)]
pub struct MapProber {
    pub map: std::collections::HashMap<String, (ProbeStatus, LoginStatus)>,
}

impl Prober for MapProber {
    fn probe_bot(&self, bot: &NotifyBotTarget) -> ProbeResult {
        let (status, login) = self
            .map
            .get(&bot.bot_id)
            .copied()
            .unwrap_or((ProbeStatus::Unknown, LoginStatus::Unknown));
        ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::ProcessMatch,
            status,
            login,
            detail: "map prober".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_prober_returns_configured() {
        let mut m = MapProber::default();
        m.map
            .insert("1".into(), (ProbeStatus::Online, LoginStatus::LoggedIn));
        let bot = NotifyBotTarget {
            bot_id: "1".into(),
            qq_id: 1,
            bot_name: String::new(),
            backend: "napcat".into(),
            deployment: "native".into(),
            container_name: None,
            pid_file: None,
            process_match: Some("x".into()),
            webui_port: None,
            webui_token: None,
            enabled: true,
        };
        let r = m.probe_bot(&bot);
        assert_eq!(r.status, ProbeStatus::Online);
        assert_eq!(r.login, LoginStatus::LoggedIn);
    }
}
