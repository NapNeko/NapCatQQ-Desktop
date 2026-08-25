//! 远端 Linux SnowLuma 发行物：完整包（自带 node）或 lite（需外置 Node）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 官方 Linux 资源：
/// - Full：`SnowLuma-<tag>-linux-*.tar.gz`，自带 node
/// - Lite：`SnowLuma-<tag>-linux-*-lite.tar.gz`，需 Node 22.13+
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum SnowLumaLinuxPackage {
    #[default]
    Full,
    Lite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_names() {
        assert_eq!(
            serde_json::to_string(&SnowLumaLinuxPackage::Full).unwrap(),
            "\"full\""
        );
        assert_eq!(
            serde_json::to_string(&SnowLumaLinuxPackage::Lite).unwrap(),
            "\"lite\""
        );
    }
}
