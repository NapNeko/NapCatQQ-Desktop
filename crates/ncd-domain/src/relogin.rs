// OneBot 重登通知配置；旧配置保持关闭。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ReloginSettings {
    pub enabled: bool,
    #[ts(type = "Array<number>")]
    pub administrator_ids: Vec<u64>,
    /// 空列表表示所有已开启掉线通知的账号。
    pub bot_ids: Vec<String>,
    pub offline_grace_seconds: u32,
    pub refresh_cooldown_seconds: u32,
}

impl Default for ReloginSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            administrator_ids: Vec::new(),
            bot_ids: Vec::new(),
            offline_grace_seconds: 60,
            refresh_cooldown_seconds: 30,
        }
    }
}

impl ReloginSettings {
    pub fn normalize(&mut self) {
        self.administrator_ids.retain(|id| *id > 0);
        self.administrator_ids.sort_unstable();
        self.administrator_ids.dedup();
        self.administrator_ids.truncate(32);
        self.bot_ids = self
            .bot_ids
            .iter()
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty())
            .collect();
        self.bot_ids.sort();
        self.bot_ids.dedup();
        self.bot_ids.truncate(256);
        self.offline_grace_seconds = self.offline_grace_seconds.clamp(15, 600);
        self.refresh_cooldown_seconds = self.refresh_cooldown_seconds.clamp(15, 300);
    }

    pub fn monitors(&self, bot: &str) -> bool {
        self.enabled
            && !self.administrator_ids.is_empty()
            && (self.bot_ids.is_empty() || self.bot_ids.iter().any(|id| id == bot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_config_does_not_enable_qr_delivery() {
        let mut settings: ReloginSettings = serde_json::from_str("{}").unwrap();
        assert!(!settings.monitors("123"));
        settings.enabled = true;
        assert!(!settings.monitors("123"));
        settings.administrator_ids = vec![0, 10, 10];
        settings.bot_ids = vec![" 123 ".into()];
        settings.normalize();
        assert_eq!(settings.administrator_ids, vec![10]);
        assert!(settings.monitors("123"));
        assert!(!settings.monitors("456"));
    }
}
