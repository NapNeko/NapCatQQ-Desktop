//! 掉线边沿去重与防抖

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::probe::ProbeStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeAction {
    /// 新出现的 offline 边沿,应告警
    FireOffline,
    /// 仍 offline,但在防抖窗内,跳过
    Debounced,
    /// online / 无变化 / 首次观测到已 offline 不告警(冷启动不刷历史掉线)
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EdgeStateFile {
    /// bot_id → 上次观测到的是否 online
    last_online: HashMap<String, bool>,
}

#[derive(Debug, Default)]
pub struct EdgeTracker {
    last_online: HashMap<String, bool>,
    last_offline_fire: HashMap<String, Instant>,
    debounce: Duration,
}

impl EdgeTracker {
    pub fn new(debounce_secs: u32) -> Self {
        Self {
            last_online: HashMap::new(),
            last_offline_fire: HashMap::new(),
            debounce: Duration::from_secs(u64::from(debounce_secs)),
            ..Default::default()
        }
    }

    pub fn load(path: &Path, debounce_secs: u32) -> Self {
        let mut t = Self::new(debounce_secs);
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(file) = serde_json::from_str::<EdgeStateFile>(&text) {
                t.last_online = file.last_online;
            }
        }
        t
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = EdgeStateFile {
            last_online: self.last_online.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// 根据本次探活结果决定是否触发 offline 告警
    pub fn observe(&mut self, bot_id: &str, status: ProbeStatus) -> EdgeAction {
        let online = matches!(status, ProbeStatus::Online);
        let prev = self.last_online.insert(bot_id.to_string(), online);

        match (prev, online) {
            // 冷启动:第一次见到已 offline,不告警(避免 Desktop 退出瞬间全员刷屏)
            (None, false) => EdgeAction::None,
            // true → false
            (Some(true), false) => self.try_fire(bot_id),
            // 持续 offline:不重复报,除非调用方另有 recovered 逻辑
            (Some(false), false) => EdgeAction::None,
            // false → true 或首次 online:不在此发 recovered(第一期可选)
            (_, true) => EdgeAction::None,
        }
    }

    fn try_fire(&mut self, bot_id: &str) -> EdgeAction {
        if !self.debounce.is_zero() {
            if let Some(prev) = self.last_offline_fire.get(bot_id) {
                if prev.elapsed() < self.debounce {
                    return EdgeAction::Debounced;
                }
            }
        }
        self.last_offline_fire
            .insert(bot_id.to_string(), Instant::now());
        EdgeAction::FireOffline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_offline_does_not_fire() {
        let mut t = EdgeTracker::new(0);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::None
        );
    }

    #[test]
    fn online_to_offline_fires() {
        let mut t = EdgeTracker::new(0);
        assert_eq!(t.observe("1", ProbeStatus::Online), EdgeAction::None);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::FireOffline
        );
    }

    #[test]
    fn debounce_suppresses_second_fire() {
        let mut t = EdgeTracker::new(60);
        t.observe("1", ProbeStatus::Online);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::FireOffline
        );
        t.observe("1", ProbeStatus::Online);
        assert_eq!(
            t.observe("1", ProbeStatus::Offline),
            EdgeAction::Debounced
        );
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
            EdgeAction::FireOffline
        );
    }
}
