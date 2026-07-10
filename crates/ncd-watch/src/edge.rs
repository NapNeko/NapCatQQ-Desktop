//! 掉线边沿去重与防抖(进程层 + 登录层)

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::probe::{LoginStatus, ProbeStatus};

/// 触发告警的掉线类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineEdgeKind {
    /// 进程/容器从 online → offline
    Process,
    /// 进程仍在,登录从 LoggedIn → LoggedOut
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeAction {
    FireOffline(OfflineEdgeKind),
    Debounced,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EdgeStateFile {
    /// bot_id → 上次进程是否 online
    #[serde(default, alias = "lastOnline")]
    last_process_online: HashMap<String, bool>,
    /// bot_id → 上次是否 LoggedIn(仅在曾观测到非 Unknown 时写入)
    #[serde(default)]
    last_login_online: HashMap<String, bool>,
}

#[derive(Debug, Default)]
pub struct EdgeTracker {
    last_process_online: HashMap<String, bool>,
    last_login_online: HashMap<String, bool>,
    last_offline_fire: HashMap<String, Instant>,
    debounce: Duration,
}

impl EdgeTracker {
    pub fn new(debounce_secs: u32) -> Self {
        Self {
            last_process_online: HashMap::new(),
            last_login_online: HashMap::new(),
            last_offline_fire: HashMap::new(),
            debounce: Duration::from_secs(u64::from(debounce_secs)),
        }
    }

    pub fn load(path: &Path, debounce_secs: u32) -> Self {
        let mut t = Self::new(debounce_secs);
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(file) = serde_json::from_str::<EdgeStateFile>(&text) {
                t.last_process_online = file.last_process_online;
                t.last_login_online = file.last_login_online;
            }
        }
        t
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = EdgeStateFile {
            last_process_online: self.last_process_online.clone(),
            last_login_online: self.last_login_online.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// 兼容旧单层 API:只看进程层,登录固定 Unknown
    pub fn observe(&mut self, bot_id: &str, status: ProbeStatus) -> EdgeAction {
        let actions = self.observe_layers(bot_id, status, LoginStatus::Unknown);
        actions.into_iter().next().unwrap_or(EdgeAction::None)
    }

    /// 进程 + 登录双层边沿。
    ///
    /// 规则:
    /// 1. Unknown 不更新、不触发该层
    /// 2. 冷启动首次已 offline / LoggedOut 不告警
    /// 3. 同轮若进程掉线,不再报登录掉线(进程挂已覆盖)
    /// 4. 防抖按 bot 共享(任一离线边沿共用窗口)
    pub fn observe_layers(
        &mut self,
        bot_id: &str,
        process: ProbeStatus,
        login: LoginStatus,
    ) -> Vec<EdgeAction> {
        let mut out = Vec::new();

        let mut process_fired = false;
        if !matches!(process, ProbeStatus::Unknown) {
            let online = matches!(process, ProbeStatus::Online);
            let prev = self
                .last_process_online
                .insert(bot_id.to_string(), online);
            match (prev, online) {
                (None, false) => out.push(EdgeAction::None),
                (Some(true), false) => {
                    let a = self.try_fire(bot_id, OfflineEdgeKind::Process);
                    process_fired = matches!(a, EdgeAction::FireOffline(_));
                    // Debounced 也算本轮已处理进程掉线,不再叠登录
                    if matches!(a, EdgeAction::FireOffline(_) | EdgeAction::Debounced) {
                        process_fired = true;
                    }
                    out.push(a);
                }
                (Some(false), false) => {}
                (_, true) => {}
            }
        }

        if process_fired {
            return out;
        }

        if matches!(login, LoginStatus::Unknown) {
            return if out.is_empty() {
                vec![EdgeAction::None]
            } else {
                out
            };
        }

        let logged_in = matches!(login, LoginStatus::LoggedIn);
        let prev = self
            .last_login_online
            .insert(bot_id.to_string(), logged_in);
        match (prev, logged_in) {
            (None, false) => {
                if out.is_empty() {
                    out.push(EdgeAction::None);
                }
            }
            (Some(true), false) => out.push(self.try_fire(bot_id, OfflineEdgeKind::Login)),
            (Some(false), false) => {}
            (_, true) => {}
        }

        if out.is_empty() {
            out.push(EdgeAction::None);
        }
        out
    }

    fn try_fire(&mut self, bot_id: &str, kind: OfflineEdgeKind) -> EdgeAction {
        if !self.debounce.is_zero() {
            if let Some(prev) = self.last_offline_fire.get(bot_id) {
                if prev.elapsed() < self.debounce {
                    return EdgeAction::Debounced;
                }
            }
        }
        self.last_offline_fire
            .insert(bot_id.to_string(), Instant::now());
        EdgeAction::FireOffline(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_offline_does_not_fire() {
        let mut t = EdgeTracker::new(0);
        assert_eq!(t.observe("1", ProbeStatus::Offline), EdgeAction::None);
    }

    #[test]
    fn online_to_offline_fires_process() {
        let mut t = EdgeTracker::new(0);
        assert_eq!(t.observe("1", ProbeStatus::Online), EdgeAction::None);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::FireOffline(OfflineEdgeKind::Process)
        );
    }

    #[test]
    fn login_logout_fires_when_process_up() {
        let mut t = EdgeTracker::new(0);
        let a = t.observe_layers("1", ProbeStatus::Online, LoginStatus::LoggedIn);
        assert!(a.iter().all(|x| matches!(x, EdgeAction::None)));
        let a = t.observe_layers("1", ProbeStatus::Online, LoginStatus::LoggedOut);
        assert!(a.iter().any(|x| matches!(
            x,
            EdgeAction::FireOffline(OfflineEdgeKind::Login)
        )));
    }

    #[test]
    fn process_down_suppresses_login_same_round() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers("1", ProbeStatus::Online, LoginStatus::LoggedIn);
        let a = t.observe_layers("1", ProbeStatus::Offline, LoginStatus::LoggedOut);
        assert!(a.iter().any(|x| matches!(
            x,
            EdgeAction::FireOffline(OfflineEdgeKind::Process)
        )));
        assert!(!a.iter().any(|x| matches!(
            x,
            EdgeAction::FireOffline(OfflineEdgeKind::Login)
        )));
    }

    #[test]
    fn unknown_login_does_not_fire() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers("1", ProbeStatus::Online, LoginStatus::Unknown);
        let a = t.observe_layers("1", ProbeStatus::Online, LoginStatus::Unknown);
        assert!(a.iter().all(|x| matches!(x, EdgeAction::None)));
    }

    #[test]
    fn debounce_suppresses_second_fire() {
        let mut t = EdgeTracker::new(60);
        t.observe("1", ProbeStatus::Online);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::FireOffline(OfflineEdgeKind::Process)
        );
        t.observe("1", ProbeStatus::Online);
        assert_eq!(t.observe("1", ProbeStatus::Offline), EdgeAction::Debounced);
    }

    #[test]
    fn persist_last_online() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edge.json");
        let mut t = EdgeTracker::new(0);
        t.observe("9", ProbeStatus::Online);
        t.save(&path).unwrap();
        let mut t2 = EdgeTracker::load(&path, 0);
        assert_eq!(
            t2.observe("9", ProbeStatus::Offline),
            EdgeAction::FireOffline(OfflineEdgeKind::Process)
        );
    }
}
