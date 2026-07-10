//! 进程 / Docker 探活

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
    pub status: ProbeStatus,
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
        if let Some(pid_file) = bot.pid_file.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
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
            detail: "no pid_file / process_match / docker target".into(),
        }
    }
}

fn probe_docker(bot: &NotifyBotTarget) -> ProbeResult {
    let name = bot.resolved_container_name();
    // docker inspect -f '{{.State.Running}}' name
    let output = std::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", &name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase();
            let online = text == "true";
            ProbeResult {
                bot_id: bot.bot_id.clone(),
                kind: ProbeKind::Docker,
                status: if online {
                    ProbeStatus::Online
                } else {
                    ProbeStatus::Offline
                },
                detail: format!("container {name} running={text}"),
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            // 容器不存在 → offline;其它错误 → unknown
            let missing = err.contains("No such object") || err.contains("no such object");
            ProbeResult {
                bot_id: bot.bot_id.clone(),
                kind: ProbeKind::Docker,
                status: if missing {
                    ProbeStatus::Offline
                } else {
                    ProbeStatus::Unknown
                },
                detail: format!("docker inspect {name}: {}", err.trim()),
            }
        }
        Err(e) => ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::Docker,
            status: ProbeStatus::Unknown,
            detail: format!("docker not available: {e}"),
        },
    }
}

fn probe_pid_file(bot: &NotifyBotTarget, path: &Path) -> ProbeResult {
    if !path.is_file() {
        return ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::PidFile,
            status: ProbeStatus::Offline,
            detail: format!("pid file missing: {}", path.display()),
        };
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            return ProbeResult {
                bot_id: bot.bot_id.clone(),
                kind: ProbeKind::PidFile,
                status: ProbeStatus::Unknown,
                detail: format!("read pid file: {e}"),
            };
        }
    };
    let pid: i32 = match text.trim().parse() {
        Ok(p) if p > 0 => p,
        _ => {
            return ProbeResult {
                bot_id: bot.bot_id.clone(),
                kind: ProbeKind::PidFile,
                status: ProbeStatus::Offline,
                detail: "pid file empty or invalid".into(),
            };
        }
    };
    let online = pid_alive(pid);
    ProbeResult {
        bot_id: bot.bot_id.clone(),
        kind: ProbeKind::PidFile,
        status: if online {
            ProbeStatus::Online
        } else {
            ProbeStatus::Offline
        },
        detail: format!("pid={pid} alive={online}"),
    }
}

fn probe_process_match(bot: &NotifyBotTarget, pattern: &str) -> ProbeResult {
    // pgrep -f pattern; exit 0 = found
    let output = std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(out) if out.status.success() => ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::ProcessMatch,
            status: ProbeStatus::Online,
            detail: format!("pgrep -f {pattern} matched"),
        },
        Ok(_) => ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::ProcessMatch,
            status: ProbeStatus::Offline,
            detail: format!("pgrep -f {pattern} no match"),
        },
        Err(e) => ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::ProcessMatch,
            status: ProbeStatus::Unknown,
            detail: format!("pgrep unavailable: {e}"),
        },
    }
}

fn pid_alive(pid: i32) -> bool {
    // kill -0 pid
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 测试用:按 bot_id 固定返回状态
#[derive(Debug, Default)]
pub struct MapProber {
    pub map: std::collections::HashMap<String, ProbeStatus>,
}

impl Prober for MapProber {
    fn probe_bot(&self, bot: &NotifyBotTarget) -> ProbeResult {
        let status = self
            .map
            .get(&bot.bot_id)
            .copied()
            .unwrap_or(ProbeStatus::Unknown);
        ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::ProcessMatch,
            status,
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
        m.map.insert("1".into(), ProbeStatus::Online);
        let bot = NotifyBotTarget {
            bot_id: "1".into(),
            qq_id: 1,
            bot_name: String::new(),
            backend: "napcat".into(),
            deployment: "native".into(),
            container_name: None,
            pid_file: None,
            process_match: Some("x".into()),
            enabled: true,
        };
        assert_eq!(m.probe_bot(&bot).status, ProbeStatus::Online);
    }
}
