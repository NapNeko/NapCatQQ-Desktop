//! OneBot 调试台 MCP 服务的设置与运行状态。服务实现在 `ncd-mcp`，
//! 这里只放跨边界（设置落盘 / Tauri 命令）的结构。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// MCP 服务设置（app-settings.json 的 `mcp`）。
/// 容器级 default：旧版本写的设置文件没有 `mcp`，整段取默认值（关着）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct McpServerSettings {
    /// 默认关：localhost HTTP 服务随这个开关起停
    pub enabled: bool,
    /// 0 = 随机空闲端口。首次起服务时系统分配的端口会回填到这里，之后固定
    pub port: u16,
    /// 危险级工具默认拒绝；打开后降级成和有副作用同级（要确认令牌才执行）
    pub allow_dangerous: bool,
}

impl Default for McpServerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            allow_dangerous: false,
        }
    }
}

/// MCP 服务当前状态（`mcp_status` 命令的返回；不带 token）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct McpServerStatus {
    /// 设置里开着
    pub enabled: bool,
    /// 端口真的在听（开启但绑定失败时为 false）
    pub listening: bool,
    /// 实际监听的端口；没在听时为空
    pub port: Option<u16>,
    /// 上一次起服务失败的原因（绑定失败、token 存取失败）；没在听且没错过为空
    pub last_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_settings_default_off_and_partial_round_trip() {
        let parsed: McpServerSettings = serde_json::from_str("{}").expect("空对象应有默认值");
        assert!(!parsed.enabled);
        assert_eq!(parsed.port, 0);
        assert!(!parsed.allow_dangerous);

        let on = McpServerSettings {
            enabled: true,
            port: 3210,
            allow_dangerous: true,
        };
        let json = serde_json::to_string(&on).unwrap();
        assert!(json.contains(r#""allowDangerous":true"#));
        assert_eq!(serde_json::from_str::<McpServerSettings>(&json).unwrap(), on);
    }
}
