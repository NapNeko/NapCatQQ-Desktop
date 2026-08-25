//! 远端 Linux 安装库存：发现结果与用户覆盖路径。
//!
//! 只含强类型；探测脚本与选中逻辑在 ncd-runtime。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bot_config::{BackendType, DeploymentType};
use crate::snowluma_linux_package::SnowLumaLinuxPackage;

/// 库存协议版本（`RemoteInventory.v`）
pub const REMOTE_INVENTORY_VERSION: u32 = 1;

/// 用户手填的绝对 POSIX 路径覆盖
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RemotePathOverrides {
    /// 含 `opt/QQ` 的根（rootless 为 `$HOME/Napcat`，系统为 `/`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_install_base: Option<String>,
    /// 含 `napcat.mjs` 的目录
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat_root: Option<String>,
    /// 含 `index.mjs` 的 SnowLuma framework 根
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_dir: Option<String>,
    /// `node` 可执行文件
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_bin: Option<String>,
    /// ncd-watch 安装根
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ncd_watch_root: Option<String>,
}

impl RemotePathOverrides {
    pub fn is_empty(&self) -> bool {
        self.qq_install_base.is_none()
            && self.napcat_root.is_none()
            && self.snowluma_dir.is_none()
            && self.node_bin.is_none()
            && self.ncd_watch_root.is_none()
    }
}

/// 库存条目种类（serde 名对齐 ComponentId）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum RemoteInventoryKind {
    #[serde(rename = "qq")]
    Qq,
    #[serde(rename = "napcat")]
    NapCat,
    #[serde(rename = "snowluma")]
    SnowLuma,
    #[serde(rename = "nodejs")]
    NodeJs,
    #[serde(rename = "ncd_watch")]
    NcdWatch,
    #[serde(rename = "docker_container")]
    DockerContainer,
}

/// 条目来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum RemoteInventorySource {
    UserOverride,
    DesktopOwned,
    OfficialInstaller,
    SystemPackage,
    PathLookup,
    Process,
}

/// 一条发现到的安装
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RemoteInventoryItem {
    pub kind: RemoteInventoryKind,
    pub root: String,
    pub source: RemoteInventorySource,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_bin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat_mjs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_napcat_js: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_mjs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_bin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_name: Option<String>,
}

/// 库存里一条已存在的 Bot 指纹（不读配置内容，只认文件名 / 容器名）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DiscoveredRemoteBotSource {
    ConfigFile,
    RuntimeStatus,
    DockerContainer,
}

/// 在已发现的 NC / SL / Docker 上看到的一个 QQ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DiscoveredRemoteBot {
    #[ts(type = "number")]
    pub qq_id: u64,
    pub backend: BackendType,
    pub deployment: DeploymentType,
    pub source: DiscoveredRemoteBotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_name: Option<String>,
}

/// Bot 页「导入已有」列表项（相对本机 bot.json 去重）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ImportableRemoteBot {
    pub server_id: String,
    pub server_name: String,
    #[ts(type = "number")]
    pub qq_id: u64,
    pub backend: BackendType,
    pub deployment: DeploymentType,
    pub source: DiscoveredRemoteBotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_name: Option<String>,
    pub already_imported: bool,
    pub selectable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

/// 启动与组件工厂实际使用的路径
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RemoteSelectedPaths {
    pub home: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_install_base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_bin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_bin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ncd_watch_root: Option<String>,
    #[serde(default)]
    pub needs_sudo: bool,
}

/// 一次探测快照
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RemoteInventory {
    pub v: u32,
    pub probed_at: String,
    pub home: String,
    pub items: Vec<RemoteInventoryItem>,
    pub selected: RemoteSelectedPaths,
    /// 已知目录 / 容器名里扫到的 Bot；旧快照缺字段当空
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bots: Vec<DiscoveredRemoteBot>,
    /// 探测时按 selected 推断；旧档案缺字段为 None（前端按完整包处理）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_linux_package: Option<SnowLumaLinuxPackage>,
}

impl RemoteInventory {
    pub fn empty(home: impl Into<String>, probed_at: impl Into<String>) -> Self {
        let home = home.into();
        Self {
            v: REMOTE_INVENTORY_VERSION,
            probed_at: probed_at.into(),
            selected: RemoteSelectedPaths {
                home: home.clone(),
                ..RemoteSelectedPaths::default()
            },
            home,
            items: Vec::new(),
            bots: Vec::new(),
            snowluma_linux_package: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_servers_json_without_inventory_fields_deserializes_overrides_default() {
        let json = r#"{"qqInstallBase":null}"#;
        let o: RemotePathOverrides = serde_json::from_str(json).unwrap();
        assert!(o.is_empty());
    }

    #[test]
    fn inventory_round_trip_preserves_kind_renames() {
        let inv = RemoteInventory {
            v: REMOTE_INVENTORY_VERSION,
            probed_at: "2026-08-25T00:00:00Z".into(),
            home: "/home/u".into(),
            items: vec![RemoteInventoryItem {
                kind: RemoteInventoryKind::NapCat,
                root: "/home/u/Napcat/opt/QQ/resources/app/app_launcher/napcat".into(),
                source: RemoteInventorySource::OfficialInstaller,
                verified: true,
                qq_bin: None,
                napcat_mjs: Some(
                    "/home/u/Napcat/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs".into(),
                ),
                load_napcat_js: Some("/home/u/Napcat/opt/QQ/resources/app/loadNapCat.js".into()),
                index_mjs: None,
                runtime_json: None,
                node_bin: None,
                docker_name: None,
            }],
            selected: RemoteSelectedPaths {
                home: "/home/u".into(),
                qq_install_base: Some("/home/u/Napcat".into()),
                qq_bin: Some("/home/u/Napcat/opt/QQ/qq".into()),
                napcat_root: Some("/home/u/Napcat/opt/QQ/resources/app/app_launcher/napcat".into()),
                snowluma_dir: None,
                snowluma_workspace: None,
                node_bin: None,
                ncd_watch_root: None,
                needs_sudo: false,
            },
            bots: Vec::new(),
            snowluma_linux_package: None,
        };
        let json = serde_json::to_value(&inv).unwrap();
        assert_eq!(json["items"][0]["kind"], "napcat");
        assert_eq!(json["items"][0]["source"], "officialInstaller");
        let back: RemoteInventory = serde_json::from_value(json).unwrap();
        assert_eq!(back, inv);
    }

    #[test]
    fn old_inventory_json_without_bots_defaults_empty() {
        let json = serde_json::json!({
            "v": 1,
            "probedAt": "t",
            "home": "/home/u",
            "items": [],
            "selected": { "home": "/home/u", "needsSudo": false }
        });
        let inv: RemoteInventory = serde_json::from_value(json).unwrap();
        assert!(inv.bots.is_empty());
        assert!(inv.snowluma_linux_package.is_none());
    }
}
