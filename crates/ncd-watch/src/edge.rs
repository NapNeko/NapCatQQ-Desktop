//! 掉线边沿去重与防抖
//!
//! 账号在线(WebUI online/isLogin)为主信号;进程/容器仅在无 WebUI 凭据,或
//! 有凭据但本轮登录探活 Unknown 且进程 Online→Offline 时回退(进程挂掉 WebUI 也死)。
//! 与 Desktop login_poller 的 online true→false 语义对齐。

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::probe::{LoginStatus, ProbeStatus};

/// 触发告警的掉线类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineEdgeKind {
    /// 进程/容器 Online→Offline(无 WebUI 凭据,或 WebUI 不可达时的回退)
    Process,
    /// 账号态 LoggedIn→LoggedOut(WebUI online/isLogin;主路径)
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeAction {
    FireOffline(OfflineEdgeKind),
    /// offline→online(需 notify_on_recovered 才投递)
    FireRecovered,
    Debounced,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EdgeStateFile {
    #[serde(default, alias = "lastOnline")]
    last_process_online: HashMap<String, bool>,
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

    /// 仅进程层(兼容旧测试/无登录信号)
    pub fn observe(&mut self, bot_id: &str, status: ProbeStatus) -> EdgeAction {
        let actions = self.observe_layers_prefer(bot_id, status, LoginStatus::Unknown, false);
        actions.into_iter().next().unwrap_or(EdgeAction::None)
    }

    /// 账号态优先 + 进程回退。
    ///
    /// `prefer_account=true`（有 WebUI 凭据）:
    /// - 登录 LoggedIn/LoggedOut 明确:跟登录边沿;进程 Online 不代表在线
    /// - 登录 Unknown 且进程 Online→Offline:回退 Process(进程/WebUI 同死)
    ///
    /// `prefer_account=false`: 进程层为主(Docker/无 token 的 native)
    ///
    /// 共用:
    /// 1. Unknown 不更新该层快照(进程 Unknown 例外于不触发)
    /// 2. 冷启动首次已 offline / LoggedOut 不告警
    /// 3. offline→online 发 FireRecovered
    /// 4. 防抖按 bot 共享,仅作用于 offline 边沿
    pub fn observe_layers(
        &mut self,
        bot_id: &str,
        process: ProbeStatus,
        login: LoginStatus,
    ) -> Vec<EdgeAction> {
        let prefer_account = !matches!(login, LoginStatus::Unknown);
        self.observe_layers_prefer(bot_id, process, login, prefer_account)
    }

    pub fn observe_layers_prefer(
        &mut self,
        bot_id: &str,
        process: ProbeStatus,
        login: LoginStatus,
        prefer_account: bool,
    ) -> Vec<EdgeAction> {
        let mut out = Vec::new();
        let login_known = !matches!(login, LoginStatus::Unknown);

        if prefer_account {
            // 进程快照始终更新;仅在登录 Unknown 时允许进程 offline 边沿作回退。
            // 意图:进程/WebUI 同死时仍能告警。token/端口错导致长期 Unknown 时可能
            // 把进程抖动误报成 Process——host_port 修对后概率应很低。
            if !matches!(process, ProbeStatus::Unknown) {
                let online = matches!(process, ProbeStatus::Online);
                let prev = self.last_process_online.insert(bot_id.to_string(), online);
                if !login_known {
                    match (prev, online) {
                        (None, false) => {}
                        (Some(true), false) => {
                            out.push(self.try_fire_offline(bot_id, OfflineEdgeKind::Process));
                        }
                        (Some(false), true) => {
                            // 进程恢复但账号态仍未知:不发 recovered(等登录确认)
                        }
                        _ => {}
                    }
                }
            }

            if login_known {
                let logged_in = matches!(login, LoginStatus::LoggedIn);
                let prev = self.last_login_online.insert(bot_id.to_string(), logged_in);
                match (prev, logged_in) {
                    (None, false) => {}
                    (Some(true), false) => {
                        out.push(self.try_fire_offline(bot_id, OfflineEdgeKind::Login));
                    }
                    (Some(false), true) => out.push(EdgeAction::FireRecovered),
                    _ => {}
                }
            }
            return finalize(out);
        }

        // 无 WebUI 凭据:进程/容器主路径
        if !matches!(process, ProbeStatus::Unknown) {
            let online = matches!(process, ProbeStatus::Online);
            let prev = self.last_process_online.insert(bot_id.to_string(), online);
            match (prev, online) {
                (None, false) => {}
                (Some(true), false) => {
                    out.push(self.try_fire_offline(bot_id, OfflineEdgeKind::Process));
                }
                (Some(false), true) => out.push(EdgeAction::FireRecovered),
                _ => {}
            }
        }

        finalize(out)
    }

    fn try_fire_offline(&mut self, bot_id: &str, kind: OfflineEdgeKind) -> EdgeAction {
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

fn finalize(mut out: Vec<EdgeAction>) -> Vec<EdgeAction> {
    if out.is_empty() {
        out.push(EdgeAction::None);
    }
    out
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
    fn offline_to_online_fires_recovered() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe("1", ProbeStatus::Online);
        let _ = t.observe("1", ProbeStatus::Offline);
        assert_eq!(
            t.observe("1", ProbeStatus::Online),
            EdgeAction::FireRecovered
        );
    }

    #[test]
    fn account_logout_fires_login_even_if_process_up() {
        let mut t = EdgeTracker::new(0);
        let a = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        assert!(a.iter().all(|x| matches!(x, EdgeAction::None)));
        let a = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedOut, true);
        assert!(
            a.iter()
                .any(|x| matches!(x, EdgeAction::FireOffline(OfflineEdgeKind::Login)))
        );
    }

    #[test]
    fn with_account_online_process_down_does_not_fire_process() {
        // 账号仍 LoggedIn 时不发 Process(进程在线≠账号在线的对称:账号在线时不看进程掉)
        // 实际上进程掉 WebUI 通常变 Unknown;此测锁「明确 LoggedIn 时忽略进程」
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        let a = t.observe_layers_prefer("1", ProbeStatus::Offline, LoginStatus::LoggedIn, true);
        assert!(
            !a.iter()
                .any(|x| matches!(x, EdgeAction::FireOffline(OfflineEdgeKind::Process)))
        );
        assert!(a.iter().all(|x| matches!(x, EdgeAction::None)));
    }

    #[test]
    fn with_webui_process_down_and_login_unknown_fires_process() {
        // 有凭据但 WebUI 不可达 + 进程掉:回退 Process
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        let a = t.observe_layers_prefer("1", ProbeStatus::Offline, LoginStatus::Unknown, true);
        assert!(
            a.iter()
                .any(|x| matches!(x, EdgeAction::FireOffline(OfflineEdgeKind::Process)))
        );
    }

    #[test]
    fn without_account_signal_process_down_fires() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::Unknown, false);
        let a = t.observe_layers_prefer("1", ProbeStatus::Offline, LoginStatus::Unknown, false);
        assert!(
            a.iter()
                .any(|x| matches!(x, EdgeAction::FireOffline(OfflineEdgeKind::Process)))
        );
    }

    #[test]
    fn account_recovered_fires() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedOut, true);
        let a = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        assert!(a.iter().any(|x| matches!(x, EdgeAction::FireRecovered)));
    }

    #[test]
    fn debounce_suppresses_second_fire() {
        let mut t = EdgeTracker::new(60);
        let _ = t.observe("1", ProbeStatus::Online);
        assert!(matches!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::FireOffline(_)
        ));
        let _ = t.observe("1", ProbeStatus::Online);
        assert_eq!(t.observe("1", ProbeStatus::Offline), EdgeAction::Debounced);
    }

    #[test]
    fn unknown_login_does_not_fire_login() {
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        let a = t.observe_layers_prefer("1", ProbeStatus::Online, LoginStatus::Unknown, true);
        assert!(a.iter().all(|x| matches!(x, EdgeAction::None)));
    }

    #[test]
    fn persist_last_online() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edge.json");
        let mut t = EdgeTracker::new(0);
        let _ = t.observe_layers_prefer("9", ProbeStatus::Online, LoginStatus::LoggedIn, true);
        t.save(&path).unwrap();
        let t2 = EdgeTracker::load(&path, 0);
        assert_eq!(t2.last_login_online.get("9"), Some(&true));
    }
}
