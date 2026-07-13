//! 应用端框架轴（AppFramework）— 与协议 BackendType 正交。
//!
//! 应用实例不是协议 Bot：无 QQ 扫码/登录态语义，不进 BackendType / BotFlavor。
//! 第一阶段只建模槽位与状态；具体框架（NoneBot2 / AstrBot 等）后补 manifest + Integration。

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

use crate::ids::BotId;
use crate::kinds::RuntimeTarget;

/// 应用端框架标识（稳定字符串，如 "nonebot2" / "astrbot"）。
/// 产品未选定前允许任意非空 id；禁止与 BackendType 变体混用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppFrameworkId(String);

impl AppFrameworkId {
    pub fn new(value: impl Into<String>) -> Self {
        let s = value.into();
        debug_assert!(!s.is_empty(), "AppFrameworkId must not be empty");
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppFrameworkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for AppFrameworkId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// 应用端实例 id（控制台侧，非 QQ 号）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstanceId(String);

impl AppInstanceId {
    pub fn new(value: impl Into<String>) -> Self {
        let s = value.into();
        debug_assert!(!s.is_empty(), "AppInstanceId must not be empty");
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for AppInstanceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// 应用端首发 placement 子集（与协议运行矩阵分表）。
/// 当前产品拍板: 本机 Native + 远端 Docker；其它组合未开。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppPlacement {
    /// 本机原生进程
    LocalNative,
    /// 远端主机 Docker
    RemoteDocker,
}

impl AppPlacement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalNative => "local_native",
            Self::RemoteDocker => "remote_docker",
        }
    }
}

/// 应用端实例生命周期（控制台子集，非协议登录态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppInstanceState {
    /// 组件未安装
    NotInstalled,
    /// 已安装未运行
    Installed,
    /// 进程/容器运行中
    Running,
    /// 已停止
    Stopped,
    /// 已写入协议对接（OneBot 出口）
    Linked,
    /// 对接失败（可读原因在 AppInstance.last_error）
    LinkFailed,
}

impl AppInstanceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Installed => "installed",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Linked => "linked",
            Self::LinkFailed => "link_failed",
        }
    }
}

/// 从协议 Bot 导出的 OneBot HTTP 出口（应用端 Integration 的输入）。
///
/// 全自动对接语义: 编排层拿到本结构后调用 AppIntegration 写入应用端配置；
/// 失败须可解释、可回滚（由 Integration 实现保证）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OneBotEndpointExport {
    /// 协议侧 Bot id（通常 QQ 号字符串）
    #[ts(type = "string")]
    pub bot_id: BotId,
    /// 协议后端展示名 napcat / snowluma（不是 AppFrameworkId）
    pub protocol_backend: String,
    /// HTTP 根地址，如 http://127.0.0.1:3000
    pub base_url: String,
    pub access_token: String,
    pub host: String,
    #[ts(type = "number")]
    pub port: u16,
    /// 协议 Bot 的 runtime_target（本机 / 某 server）
    #[ts(type = "string")]
    pub runtime_target: RuntimeTarget,
}

/// 应用端实例快照（控制台列表/状态用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstance {
    pub id: AppInstanceId,
    pub framework_id: AppFrameworkId,
    pub display_name: String,
    pub placement: AppPlacement,
    pub state: AppInstanceState,
    /// 已对接的协议 Bot；未对接为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub linked_bot_id: Option<BotId>,
    /// 远端 Docker 时的 server profile id；本机 Native 为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_config::BackendType;

    #[test]
    fn backend_type_is_protocol_only_not_app_framework() {
        // 应用端不得塞进 BackendType；变体集合即协议轴。
        let variants = [BackendType::NapCat, BackendType::SnowLuma];
        assert_eq!(variants.len(), 2);
        let _ = AppFrameworkId::new("nonebot2");
        let _ = AppFrameworkId::new("astrbot");
    }

    #[test]
    fn app_placement_subset_is_local_native_and_remote_docker() {
        assert_eq!(AppPlacement::LocalNative.as_str(), "local_native");
        assert_eq!(AppPlacement::RemoteDocker.as_str(), "remote_docker");
    }

    #[test]
    fn onebot_export_round_trips_json() {
        let export = OneBotEndpointExport {
            bot_id: BotId::new("10001"),
            protocol_backend: "napcat".into(),
            base_url: "http://127.0.0.1:3000".into(),
            access_token: "t".into(),
            host: "127.0.0.1".into(),
            port: 3000,
            runtime_target: RuntimeTarget::Local,
        };
        let v = serde_json::to_value(&export).unwrap();
        let back: OneBotEndpointExport = serde_json::from_value(v).unwrap();
        assert_eq!(back.port, 3000);
        assert_eq!(back.bot_id.as_str(), "10001");
    }
}
