//! 应用端配置文档的框架无关部分：文档清单、内容哈希版本号、原始文本读写、类型化配置信封。
//!
//! 版本号 = 文件内容 sha256 十六进制前 16 位（缺文件为 `"missing"`）；多文件取「id=rev」拼接后的哈希。
//! 写回必须带读时的版本号，不一致视为别处改过（Karin WebUI / 手改），交给 UI 决定重载还是覆盖。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue, AppConfigText};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::adapter::apply_with_backup_ex;
use crate::astrbot::config::AstrBotInstanceConfig;
use crate::karin::config::KarinInstanceConfig;
use crate::nonebot2::config::NoneBot2InstanceConfig;

pub const MISSING_REVISION: &str = "missing";

/// 类型化配置：按框架分流（`framework` 标签与 `AppFrameworkId` 字面量一致）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "framework", content = "data", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppInstanceConfig {
    Karin(KarinInstanceConfig),
    #[serde(rename = "nonebot2")]
    NoneBot2(NoneBot2InstanceConfig),
    #[serde(rename = "astrbot")]
    AstrBot(AstrBotInstanceConfig),
}

impl AppInstanceConfig {
    /// WebUI 登录密钥；空串表示不用复制。
    pub fn webui_auth_key(&self) -> &str {
        match self {
            Self::Karin(c) => c.env.http_auth_key.as_str(),
            Self::NoneBot2(_) | Self::AstrBot(_) => "",
        }
    }

    /// 应用端监听口（编排层同步实例 `port` / 重新对接用）。
    pub fn listen_port(&self) -> u16 {
        match self {
            Self::Karin(c) => c.env.http_port,
            Self::NoneBot2(c) => c.env_prod.port,
            Self::AstrBot(c) => c.onebot.ws_reverse_port,
        }
    }

    /// WebUI HTTP 口；没有 WebUI 返回 None。AstrBot 与 OneBot 口不是同一个。
    pub fn webui_port(&self) -> Option<u16> {
        match self {
            Self::Karin(c) => Some(c.env.http_port),
            Self::NoneBot2(_) => None,
            Self::AstrBot(c) => Some(c.dashboard_port).filter(|p| *p > 0),
        }
    }

    /// 对接依赖的输入（端口 / 反向 WS 秘钥）是否变了。跨框架比较视为没变。
    pub fn link_inputs_changed(&self, after: &Self) -> bool {
        match (self, after) {
            (Self::Karin(before), Self::Karin(after)) => {
                crate::karin::config::link_inputs_changed(&before.env, &after.env)
            }
            (Self::NoneBot2(before), Self::NoneBot2(after)) => {
                crate::nonebot2::config::link_inputs_changed(&before.env_prod, &after.env_prod)
            }
            (Self::AstrBot(before), Self::AstrBot(after)) => {
                crate::astrbot::config::link_inputs_changed(before, after)
            }
            _ => false,
        }
    }
}

/// 单个文档的版本信息（信封里逐文件列出，UI 据此判断哪些改动要重启）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigDocumentRevision {
    pub doc_id: String,
    pub revision: String,
    pub hot_reload: bool,
}

/// 类型化配置 + 版本号
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstanceConfigEnvelope {
    pub config: AppInstanceConfig,
    /// 所有文档的合并版本号；`write_config` 的 `base_revision` 传这个
    pub revision: String,
    pub documents: Vec<AppConfigDocumentRevision>,
}

/// 类型化写入结果（编排层在信封之上补的联动信息）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigWriteResult {
    pub config: AppInstanceConfig,
    pub revision: String,
    pub documents: Vec<AppConfigDocumentRevision>,
    /// 改到了应用端不热加载的文件且实例正在运行
    pub restart_required: bool,
    /// 端口 / 反向 WS 秘钥变了且已对接 → 已重新写协议 Bot 侧连接
    pub relinked: bool,
    /// 实例端口已随 HTTP_PORT 同步
    pub port_changed: bool,
}

/// 校验结果的便捷收集器
#[derive(Debug, Default)]
pub struct IssueSink {
    issues: Vec<AppConfigIssue>,
}

impl IssueSink {
    pub fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(AppConfigIssue::new(path, message));
    }

    pub fn into_vec(self) -> Vec<AppConfigIssue> {
        self.issues
    }
}

pub fn revision_of(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex = format!("{digest:x}");
    hex[..16].to_string()
}

/// 多文档合并版本号：对 `id=rev\n` 逐条拼接再哈希，与文档顺序有关（调用方保证顺序稳定）
pub fn combined_revision<'a>(parts: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut hasher = Sha256::new();
    for (id, rev) in parts {
        hasher.update(id.as_bytes());
        hasher.update(b"=");
        hasher.update(rev.as_bytes());
        hasher.update(b"\n");
    }
    let hex = format!("{:x}", hasher.finalize());
    hex[..16].to_string()
}

pub fn document_path(install_dir: &HostPath, doc: &AppConfigDocument) -> HostPath {
    install_dir.join(&doc.rel_path)
}

/// 读到的一份文档（缺文件 `text = None`）
#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    pub doc: AppConfigDocument,
    pub text: Option<String>,
    pub revision: String,
}

impl DocumentSnapshot {
    pub fn revision_entry(&self) -> AppConfigDocumentRevision {
        AppConfigDocumentRevision {
            doc_id: self.doc.id.clone(),
            revision: self.revision.clone(),
            hot_reload: self.doc.hot_reload,
        }
    }
}

pub async fn read_document(
    host: &dyn Host,
    install_dir: &HostPath,
    doc: &AppConfigDocument,
) -> Result<DocumentSnapshot, AppFrameworkError> {
    let path = document_path(install_dir, doc);
    let exists = host
        .exists(&path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !exists {
        return Ok(DocumentSnapshot {
            doc: doc.clone(),
            text: None,
            revision: MISSING_REVISION.to_string(),
        });
    }
    let bytes = host
        .read_file(&path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(DocumentSnapshot {
        doc: doc.clone(),
        revision: revision_of(&bytes),
        text: Some(String::from_utf8_lossy(&bytes).into_owned()),
    })
}

pub async fn read_documents(
    host: &dyn Host,
    install_dir: &HostPath,
    docs: &[AppConfigDocument],
) -> Result<Vec<DocumentSnapshot>, AppFrameworkError> {
    let mut out = Vec::with_capacity(docs.len());
    for doc in docs {
        out.push(read_document(host, install_dir, doc).await?);
    }
    Ok(out)
}

pub fn combined_revision_of(snapshots: &[DocumentSnapshot]) -> String {
    combined_revision(
        snapshots
            .iter()
            .map(|s| (s.doc.id.as_str(), s.revision.as_str())),
    )
}

/// 原始文本写前预检：JSON / TOML 必须能解析；dotenv 不限
pub fn validate_text(format: AppConfigFormat, text: &str) -> Result<(), AppFrameworkError> {
    let err = match format {
        AppConfigFormat::Json => serde_json::from_str::<serde_json::Value>(text)
            .err()
            .map(|e| format!("JSON 语法错误: {e}")),
        AppConfigFormat::Toml => toml::from_str::<toml::Value>(text)
            .err()
            .map(|e| format!("TOML 语法错误: {e}")),
        AppConfigFormat::DotEnv => None,
    };
    match err {
        Some(message) => Err(AppFrameworkError::ConfigInvalid(vec![AppConfigIssue::new(
            "text", message,
        )])),
        None => Ok(()),
    }
}

/// 写一份文档原文：预检 → 建父目录 → 备份写入 → 返回新版本号。
/// `base_revision` 为 Some 时先比对当前版本，不一致返回 `ConfigConflict`。
pub async fn write_document_text(
    host: &dyn Host,
    install_dir: &HostPath,
    doc: &AppConfigDocument,
    text: &str,
    base_revision: Option<&str>,
    write_sidecar: bool,
) -> Result<AppConfigText, AppFrameworkError> {
    validate_text(doc.format, text)?;
    if let Some(base) = base_revision {
        let current = read_document(host, install_dir, doc).await?;
        if current.revision != base {
            return Err(AppFrameworkError::ConfigConflict(doc.id.clone()));
        }
    }
    let path = document_path(install_dir, doc);
    ensure_parent_dir(host, &path).await?;
    let bytes = text.as_bytes().to_vec();
    apply_with_backup_ex(host, std::slice::from_ref(&path), write_sidecar, || async {
        host.write_file(&path, &bytes)
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))
    })
    .await?;
    Ok(AppConfigText {
        doc_id: doc.id.clone(),
        text: text.to_string(),
        revision: revision_of(&bytes),
    })
}

pub async fn ensure_parent_dir(host: &dyn Host, path: &HostPath) -> Result<(), AppFrameworkError> {
    if let Some(parent) = path.parent() {
        let exists = host
            .exists(&parent)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if !exists {
            host.create_dir_all(&parent)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        }
    }
    Ok(())
}

/// 把 JSON Value 渲染成 Karin / 人手都好读的两空格缩进 + 末尾换行
pub fn render_json_pretty<T: Serialize>(value: &T) -> Result<String, AppFrameworkError> {
    serde_json::to_string_pretty(value)
        .map(|s| format!("{s}\n"))
        .map_err(|e| AppFrameworkError::Integration(format!("序列化配置失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::karin::config::KarinInstanceConfig;

    #[test]
    fn nonebot2_framework_tag_is_literal_id() {
        let cfg = AppInstanceConfig::NoneBot2(crate::nonebot2::config::NoneBot2InstanceConfig {
            env_prod: crate::nonebot2::config::NoneBot2EnvProd::default(),
        });
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(v["framework"], "nonebot2");
    }

    #[test]
    fn webui_auth_key_reads_karin_http_auth() {
        let mut cfg = KarinInstanceConfig::upstream_default();
        assert_eq!(AppInstanceConfig::Karin(cfg.clone()).webui_auth_key(), "");
        cfg.env.http_auth_key = "secret-1".into();
        assert_eq!(AppInstanceConfig::Karin(cfg).webui_auth_key(), "secret-1");
    }

    #[test]
    fn listen_port_reads_karin_http_port() {
        let mut cfg = KarinInstanceConfig::upstream_default();
        assert_eq!(AppInstanceConfig::Karin(cfg.clone()).listen_port(), 7777);
        cfg.env.http_port = 7801;
        assert_eq!(AppInstanceConfig::Karin(cfg).listen_port(), 7801);
    }

    #[test]
    fn listen_port_reads_astrbot_onebot_port() {
        let mut cfg = crate::astrbot::AstrBotInstanceConfig::default();
        assert_eq!(
            AppInstanceConfig::AstrBot(cfg.clone()).listen_port(),
            6199
        );
        cfg.onebot.ws_reverse_port = 6201;
        assert_eq!(AppInstanceConfig::AstrBot(cfg.clone()).listen_port(), 6201);
        assert_eq!(
            AppInstanceConfig::AstrBot(cfg).webui_port(),
            Some(6185)
        );
        assert_eq!(
            AppInstanceConfig::AstrBot(crate::astrbot::AstrBotInstanceConfig::default())
                .webui_auth_key(),
            ""
        );
    }

    #[test]
    fn listen_port_reads_nonebot2_env_prod_port() {
        let mut env = crate::nonebot2::config::NoneBot2EnvProd::default();
        assert_eq!(
            AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
                env_prod: env.clone()
            })
            .listen_port(),
            8080
        );
        env.port = 9090;
        assert_eq!(
            AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig { env_prod: env })
                .listen_port(),
            9090
        );
    }

    #[test]
    fn link_inputs_changed_tracks_karin_port_and_ws_key() {
        let a = AppInstanceConfig::Karin(KarinInstanceConfig::upstream_default());
        let mut b_cfg = KarinInstanceConfig::upstream_default();
        assert!(!a.link_inputs_changed(&AppInstanceConfig::Karin(b_cfg.clone())));
        b_cfg.env.http_port = 8000;
        assert!(a.link_inputs_changed(&AppInstanceConfig::Karin(b_cfg.clone())));
        b_cfg = KarinInstanceConfig::upstream_default();
        b_cfg.env.ws_server_auth_key = "x".into();
        assert!(a.link_inputs_changed(&AppInstanceConfig::Karin(b_cfg.clone())));
        b_cfg = KarinInstanceConfig::upstream_default();
        b_cfg.env.log_level = "debug".into();
        assert!(!a.link_inputs_changed(&AppInstanceConfig::Karin(b_cfg)));
    }

    #[test]
    fn link_inputs_changed_tracks_nonebot2_port_and_token() {
        let a = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: crate::nonebot2::config::NoneBot2EnvProd::default(),
        });
        let mut env = crate::nonebot2::config::NoneBot2EnvProd::default();
        let same = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: env.clone(),
        });
        assert!(!a.link_inputs_changed(&same));
        env.port = 9091;
        let port = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: env.clone(),
        });
        assert!(a.link_inputs_changed(&port));
        env = crate::nonebot2::config::NoneBot2EnvProd::default();
        env.onebot_access_token = "tok".into();
        let token = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: env,
        });
        assert!(a.link_inputs_changed(&token));
    }

    #[test]
    fn link_inputs_changed_is_false_across_frameworks() {
        let karin = AppInstanceConfig::Karin(KarinInstanceConfig::upstream_default());
        let nb2 = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: crate::nonebot2::config::NoneBot2EnvProd::default(),
        });
        assert!(!karin.link_inputs_changed(&nb2));
    }

    #[test]
    fn revision_is_stable_and_short() {
        let a = revision_of(b"hello");
        assert_eq!(a.len(), 16);
        assert_eq!(a, revision_of(b"hello"));
        assert_ne!(a, revision_of(b"hello\n"));
    }

    #[test]
    fn combined_revision_depends_on_each_part() {
        let base = combined_revision([("env", "aaaa"), ("config", "bbbb")]);
        assert_eq!(base, combined_revision([("env", "aaaa"), ("config", "bbbb")]));
        assert_ne!(base, combined_revision([("env", "aaaa"), ("config", "cccc")]));
        assert_ne!(base, combined_revision([("config", "bbbb"), ("env", "aaaa")]));
    }

    #[test]
    fn validate_text_checks_json_and_toml_only() {
        assert!(validate_text(AppConfigFormat::Json, "{\"a\":1}").is_ok());
        assert!(matches!(
            validate_text(AppConfigFormat::Json, "{oops"),
            Err(AppFrameworkError::ConfigInvalid(_))
        ));
        assert!(validate_text(AppConfigFormat::Toml, "[tool]\nname = \"x\"\n").is_ok());
        assert!(matches!(
            validate_text(AppConfigFormat::Toml, "= broken"),
            Err(AppFrameworkError::ConfigInvalid(_))
        ));
        assert!(validate_text(AppConfigFormat::DotEnv, "whatever = = =").is_ok());
    }
}
