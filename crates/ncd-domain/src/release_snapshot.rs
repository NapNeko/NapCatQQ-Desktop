//! GitHub releases 拉取的远端版本快照, Home 页 update notice 派生用
//!
//! 拉取实装在 crates/ncd-runtime/src/release.rs, 本文件只放跨边界数据契约
//! 任何字段都允许为 None(网络失败 / 解析失败 / 还没拉过), 前端把任一
//! 字段 None 当作"暂不显示对应 update notice"

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 单个 release asset 的指纹条目
///
/// sha256 来源是 GitHub Releases API 的 assets[*].digest 字段
/// ("sha256:<64-hex>" 形态, 由 runtime 层剥前缀), 用于安装前下载完整性校验
/// GitHub 没给 digest(老仓库 / 老 release)时该字段为空串,
/// 前端 / 安装层应当当作"无 hash 数据"处理
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ReleaseAsset {
    /// asset 文件名, 与 release URL 路径末尾一致(如 NapCat.Shell.zip /
    /// SnowLuma-v1.9.2-win-x64.zip), 安装层按文件名反查指纹
    pub name: String,
    /// 64 位 hex 小写 SHA256;GitHub 没给 digest 时为空串
    pub sha256: String,
}

/// 单个项目的 release 元数据(NapCat / SnowLuma / Desktop 共用结构)
///
/// 字段:
/// - version: tag_name 去 v 前缀后的版本号字面量, 保留 release 自身写法, 不强制 SemVer
/// - tag: 原始 tag_name(含 v 前缀, 如 v1.9.2), 下载 URL 拼接需要原始 tag
/// - published_at: 发布时间 Unix epoch 秒, GitHub 返回 ISO8601, 由 runtime 层转换
/// - html_url: release 详情页 URL, 前端"查看更新"按钮直接跳转
/// - release_notes: release notes 全文(GitHub body 字段), 可能多行 Markdown
/// - assets: release 资产指纹列表(含 sha256), 安装层下载完成后做完整性校验防代理投毒
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ReleaseInfo {
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    pub published_at: u64,
    pub html_url: String,
    pub release_notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ReleaseAsset>,
}

/// 一次拉取远端 releases 的快照结果
///
/// 任一 *_latest 为 None 表示对应仓库本次未成功拉到(网络 / 解析失败 / 仓库未配置),
/// 前端单独按字段降级, fetched_at 为 None 表示从未成功拉取
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ReleaseSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat_latest: Option<ReleaseInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_latest: Option<ReleaseInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_latest: Option<ReleaseInfo>,
    /// 本快照拉取的时间戳 Unix epoch 秒, None 表示从未成功拉取
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_release() -> ReleaseInfo {
        ReleaseInfo {
            version: "4.18.1".to_string(),
            tag: "v4.18.1".to_string(),
            published_at: 1_700_000_000,
            html_url: "https://github.com/NapNeko/NapCatQQ/releases/tag/v4.18.1".to_string(),
            release_notes: "fix: foo\nfeat: bar".to_string(),
            assets: vec![ReleaseAsset {
                name: "NapCat.Shell.zip".to_string(),
                sha256: "abc123def4567890abc123def4567890abc123def4567890abc123def4567890"
                    .to_string(),
            }],
        }
    }

    /// 默认值序列化为空对象(所有 Option None + skip), 保证缓存层即使
    /// 写入空快照也能稳定反序列化回 Default
    #[test]
    fn default_snapshot_round_trips_as_empty_object() {
        let value = ReleaseSnapshot::default();
        let serialized = serde_json::to_string(&value).expect("serialize default");
        assert_eq!(serialized, "{}");

        let decoded: ReleaseSnapshot =
            serde_json::from_str(&serialized).expect("deserialize default");
        assert_eq!(decoded, value);
    }

    #[test]
    fn populated_snapshot_round_trips_byte_stable() {
        let value = ReleaseSnapshot {
            napcat_latest: Some(sample_release()),
            snowluma_latest: None,
            desktop_latest: None,
            fetched_at: Some(1_700_000_500),
        };
        let serialized = serde_json::to_string(&value).expect("serialize populated");
        let decoded: ReleaseSnapshot =
            serde_json::from_str(&serialized).expect("deserialize populated");
        assert_eq!(decoded, value);
    }

    #[test]
    fn release_info_round_trips() {
        let value = sample_release();
        let serialized = serde_json::to_string(&value).expect("serialize info");
        let decoded: ReleaseInfo = serde_json::from_str(&serialized).expect("deserialize info");
        assert_eq!(decoded, value);
    }

    /// 老缓存(无 assets / 无 tag 字段)必须能反序列化回 ReleaseInfo,
    /// assets 走空 Vec 默认值, tag 走空串默认值, 保证新 sha256 字段上线不破坏已有缓存
    #[test]
    fn legacy_release_info_without_assets_and_tag_deserializes() {
        let legacy = r#"{
            "version": "4.18.1",
            "published_at": 1700000000,
            "html_url": "https://example.com",
            "release_notes": "x"
        }"#;
        let decoded: ReleaseInfo = serde_json::from_str(legacy).expect("deserialize legacy");
        assert_eq!(decoded.version, "4.18.1");
        assert_eq!(decoded.tag, "");
        assert!(decoded.assets.is_empty());
    }
}
