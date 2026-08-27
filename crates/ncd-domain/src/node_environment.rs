//! Node.js 运行环境候选项与探测结果契约（Layer 1）

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Node.js 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum NodeSourceKind {
    /// SnowLuma 自身解压包内置的 node
    Bundled,
    /// 独立 Node.js 组件（由 NapCatQQ Desktop 独立下载安装）
    Component,
    /// 环境变量 PATH 中的 node
    SystemPath,
    /// 版本管理器（如 NVM, fnm, Volta）或 Program Files 常见路径
    VersionManager,
    /// 用户在高级设置中指定的自定义路径
    Custom,
}

/// 探测到的单个 Node.js 运行环境候选项
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NodeEnvironmentCandidate {
    /// 可执行文件绝对路径
    pub path: String,
    /// 探测到的版本号（无 v 前缀，如 "22.14.0"）
    pub version: String,
    /// 来源类型
    pub source_kind: NodeSourceKind,
    /// 界面展示的友好标签（如 "SnowLuma 内置 (v22.14.0)"、"系统 PATH (v22.13.1)"）
    pub label: String,
    /// 是否满足 SnowLuma 官方版本要求（^22.13.0 || >=23.4.0）
    pub is_valid: bool,
}

/// 指定二进制路径的单次探测结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NodeProbeResult {
    /// 可执行文件绝对路径
    pub path: String,
    /// 是否存在且可执行
    pub exists: bool,
    /// 探测到的版本号（若成功）
    pub version: Option<String>,
    /// 是否满足 SnowLuma 官方版本要求
    pub is_valid: bool,
    /// 错误信息（若探测失败）
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_node_source_kind() {
        assert_eq!(
            serde_json::to_string(&NodeSourceKind::Bundled).unwrap(),
            "\"bundled\""
        );
        assert_eq!(
            serde_json::to_string(&NodeSourceKind::Component).unwrap(),
            "\"component\""
        );
        assert_eq!(
            serde_json::to_string(&NodeSourceKind::SystemPath).unwrap(),
            "\"system_path\""
        );
        assert_eq!(
            serde_json::to_string(&NodeSourceKind::VersionManager).unwrap(),
            "\"version_manager\""
        );
        assert_eq!(
            serde_json::to_string(&NodeSourceKind::Custom).unwrap(),
            "\"custom\""
        );
    }
}
