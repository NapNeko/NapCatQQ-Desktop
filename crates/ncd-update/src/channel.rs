//! 发布通道(stable / beta / nightly)。
//!
//! 多通道复用同一 endpoint URL 模板,通过 {channel} 占位符替换。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl UpdateChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }

    /// 默认通道(stable)。
    pub fn default_stable() -> Self {
        Self::Stable
    }
}

impl Default for UpdateChannel {
    fn default() -> Self {
        Self::Stable
    }
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_serialize_snake_case() {
        assert_eq!(serde_json::to_string(&UpdateChannel::Stable).unwrap(), "\"stable\"");
        assert_eq!(serde_json::to_string(&UpdateChannel::Beta).unwrap(), "\"beta\"");
        assert_eq!(serde_json::to_string(&UpdateChannel::Nightly).unwrap(), "\"nightly\"");
    }

    #[test]
    fn channels_round_trip_via_json() {
        for ch in [UpdateChannel::Stable, UpdateChannel::Beta, UpdateChannel::Nightly] {
            let s = serde_json::to_string(&ch).unwrap();
            let back: UpdateChannel = serde_json::from_str(&s).unwrap();
            assert_eq!(ch, back);
        }
    }

    #[test]
    fn default_is_stable() {
        assert_eq!(UpdateChannel::default(), UpdateChannel::Stable);
    }

    #[test]
    fn as_str_matches_serde() {
        assert_eq!(UpdateChannel::Stable.as_str(), "stable");
        assert_eq!(UpdateChannel::Nightly.as_str(), "nightly");
    }

    #[test]
    fn display_uses_lowercase() {
        assert_eq!(format!("{}", UpdateChannel::Beta), "beta");
    }
}
