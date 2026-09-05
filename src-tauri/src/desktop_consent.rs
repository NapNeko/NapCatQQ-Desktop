//! Desktop 用户协议 / 隐私说明：正文 embed + 同意记录。
//!
//! 与 SnowLuma runtime consent 独立：落盘在 Desktop data_root/config/desktop-consent.json。
//! 同意键取各文档头部声明的版本号（`版本 / Version:`），只有维护者显式升版才要求重签；
//! 早期版本用正文 content-hash 当键，导致同一份文本在 LF / CRLF 两种 checkout 下算出
//! 不同指纹，换个构建产物就重复弹窗，故保留旧 hash 的兼容识别。
//! 文档正文与指纹进程内缓存，避免每次 IPC 复制整份 Markdown。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

const EULA_MD: &str = include_str!("../legal/EULA.md");
const PRIVACY_MD: &str = include_str!("../legal/PRIVACY.md");
const CONSENT_FILE: &str = "desktop-consent.json";

/// 前端 / 日志可识别的未同意错误前缀（与 SNOWLUMA_CONSENT_REQUIRED 同风格）。
pub const DESKTOP_CONSENT_REQUIRED_PREFIX: &str = "DESKTOP_CONSENT_REQUIRED";
/// 客户端提交的 version 与当前正文 hash 不一致。
pub const DESKTOP_CONSENT_VERSION_MISMATCH_PREFIX: &str = "DESKTOP_CONSENT_VERSION_MISMATCH";

#[derive(Debug, thiserror::Error)]
pub enum DesktopConsentError {
    #[error("create {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("rename {from} to {to}: {source}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    #[error("remove {path}: {source}")]
    Remove {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{DESKTOP_CONSENT_VERSION_MISMATCH_PREFIX}: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    #[error(
        "{DESKTOP_CONSENT_REQUIRED_PREFIX}: desktop agreements version {version} requires consent"
    )]
    ConsentRequired { version: String },
    #[error("serialize consent record: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl DesktopConsentError {
    /// 给 Tauri command 用的稳定错误串。
    pub fn to_command_string(&self) -> String {
        match self {
            Self::VersionMismatch { expected, actual } => format!(
                "{DESKTOP_CONSENT_VERSION_MISMATCH_PREFIX}: expected {expected}, got {actual}"
            ),
            Self::ConsentRequired { version } => format!(
                "{DESKTOP_CONSENT_REQUIRED_PREFIX}: desktop agreements version {version} requires consent"
            ),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct DesktopAgreementDoc {
    pub id: String,
    pub title: String,
    pub declared_version: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct DesktopAgreementsPayload {
    pub version: String,
    pub consent_required: bool,
    pub documents: Vec<DesktopAgreementDoc>,
    /// 磁盘上记录的同意时间（ISO）。
    /// 当 consent_required 为 true 时可能仍有值，表示历史版本曾同意，不代表当前正文已生效。
    pub accepted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConsentRecord {
    version: String,
    #[serde(rename = "acceptedAt")]
    accepted_at: String,
}

#[derive(Debug, Serialize)]
struct NewConsentRecord<'a> {
    version: &'a str,
    /// 仅供排查正文漂移，不参与门禁判定。
    #[serde(rename = "contentHash")]
    content_hash: &'a str,
    #[serde(rename = "acceptedAt")]
    accepted_at: String,
}

struct CachedAgreements {
    documents: Vec<DesktopAgreementDoc>,
    version: String,
    content_hash: String,
    /// 旧版 content-hash 键：同一份正文在 LF / CRLF 两种 checkout 下的取值。
    legacy_versions: Vec<String>,
}

fn cached_agreements() -> &'static CachedAgreements {
    static CACHE: OnceLock<CachedAgreements> = OnceLock::new();
    CACHE.get_or_init(|| {
        let documents = load_documents_uncached();
        let version = compute_version_key(&documents);
        let content_hash = compute_content_hash(&documents);
        let crlf_docs: Vec<DesktopAgreementDoc> = documents
            .iter()
            .map(|doc| DesktopAgreementDoc {
                text: to_crlf(&doc.text),
                ..doc.clone()
            })
            .collect();
        CachedAgreements {
            legacy_versions: vec![content_hash.clone(), compute_content_hash(&crlf_docs)],
            documents,
            version,
            content_hash,
        }
    })
}

/// 当前同意键：各协议声明版本的组合（如 `eula@1.3+privacy@1.2`）。
pub fn current_version() -> &'static str {
    cached_agreements().version.as_str()
}

pub fn load_payload(data_root: &Path) -> DesktopAgreementsPayload {
    let cache = cached_agreements();
    let stored = read_consent_record(data_root);
    let consent_required = !stored.as_ref().is_some_and(is_accepted_record);
    DesktopAgreementsPayload {
        version: cache.version.clone(),
        consent_required,
        documents: cache.documents.clone(),
        accepted_at: stored.map(|r| r.accepted_at),
    }
}

/// 是否仍需用户确认当前协议版本。
pub fn is_consent_required(data_root: &Path) -> bool {
    !read_consent_record(data_root)
        .as_ref()
        .is_some_and(is_accepted_record)
}

fn is_accepted_record(record: &ConsentRecord) -> bool {
    let cache = cached_agreements();
    let stored = record.version.trim();
    stored == cache.version || cache.legacy_versions.iter().any(|legacy| legacy == stored)
}

/// 关键操作前调用：未同意则 Err(ConsentRequired)。
pub fn ensure_accepted(data_root: &Path) -> Result<(), DesktopConsentError> {
    if !is_consent_required(data_root) {
        return Ok(());
    }
    Err(DesktopConsentError::ConsentRequired {
        version: current_version().to_string(),
    })
}

pub fn record_consent(data_root: &Path, version: &str) -> Result<(), DesktopConsentError> {
    let current = current_version();
    if current != version {
        return Err(DesktopConsentError::VersionMismatch {
            expected: current.to_string(),
            actual: version.to_string(),
        });
    }

    let config_dir = data_root.join("config");
    fs::create_dir_all(&config_dir).map_err(|source| DesktopConsentError::CreateDir {
        path: config_dir.clone(),
        source,
    })?;

    let path = config_dir.join(CONSENT_FILE);
    let tmp = config_dir.join(format!("{CONSENT_FILE}.tmp"));
    let record = NewConsentRecord {
        version,
        content_hash: cached_agreements().content_hash.as_str(),
        accepted_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let bytes = serde_json::to_vec_pretty(&record)?;
    fs::write(&tmp, bytes).map_err(|source| DesktopConsentError::Write {
        path: tmp.clone(),
        source,
    })?;
    replace_file(&tmp, &path)
}

fn load_documents_uncached() -> Vec<DesktopAgreementDoc> {
    [("eula", EULA_MD), ("privacy", PRIVACY_MD)]
        .into_iter()
        .map(|(id, raw)| {
            // 归一化换行：Windows checkout 可能把仓库里的 LF 转成 CRLF，
            // 正文相同却算出不同指纹
            let text = normalize_text(raw);
            let meta = parse_agreement_meta(&text);
            DesktopAgreementDoc {
                id: id.to_string(),
                title: meta.title,
                declared_version: meta.declared_version,
                // include_str 正文在进程内只构建一次（经 OnceLock）
                text,
            }
        })
        .collect()
}

fn normalize_text(raw: &str) -> String {
    raw.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn to_crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

fn read_consent_record(data_root: &Path) -> Option<ConsentRecord> {
    let path = data_root.join("config").join(CONSENT_FILE);
    let text = fs::read_to_string(path).ok()?;
    let record: ConsentRecord = serde_json::from_str(&text).ok()?;
    let trimmed = record.version.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(ConsentRecord {
        version: trimmed.to_string(),
        accepted_at: record.accepted_at,
    })
}

/// 原子替换：Windows 上目标已存在时 rename 常失败，先删再 rename。
fn replace_file(from: &Path, to: &Path) -> Result<(), DesktopConsentError> {
    if to.exists() {
        fs::remove_file(to).map_err(|source| DesktopConsentError::Remove {
            path: to.to_path_buf(),
            source,
        })?;
    }
    fs::rename(from, to).map_err(|source| DesktopConsentError::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    })
}

/// 同意键：只跟随文档声明版本变化。改错别字不该让所有用户重签，改条款则必须升版。
fn compute_version_key(docs: &[DesktopAgreementDoc]) -> String {
    docs.iter()
        .map(|doc| {
            let version = if doc.declared_version.is_empty() {
                // 头部没写版本号时退回单篇指纹，避免键退化成空串静默放行
                format!("sha-{}", hash_hex16(&[doc.text.as_str()]))
            } else {
                doc.declared_version.clone()
            };
            format!("{}@{version}", doc.id)
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// 正文指纹：布局须与旧版同意键一致，否则老记录认不出来。
fn compute_content_hash(docs: &[DesktopAgreementDoc]) -> String {
    let parts: Vec<&str> = docs
        .iter()
        .flat_map(|doc| [doc.id.as_str(), doc.text.as_str()])
        .collect();
    hash_hex16(&parts)
}

fn hash_hex16(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update(b"\0");
    }
    let hex = hex::encode(hash.finalize());
    hex.get(..16).unwrap_or(hex.as_str()).to_string()
}

struct AgreementMeta {
    title: String,
    declared_version: String,
}

fn parse_agreement_meta(text: &str) -> AgreementMeta {
    AgreementMeta {
        title: parse_title(text),
        declared_version: parse_declared_version(text),
    }
}

fn parse_title(text: &str) -> String {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("# ").map(str::trim))
        .unwrap_or_default()
        .to_string()
}

fn parse_declared_version(text: &str) -> String {
    for line in text.lines() {
        if !(line.contains("Version") || line.contains("版本")) {
            continue;
        }
        let Some(start) = line.find(|c: char| c.is_ascii_digit()) else {
            continue;
        };
        let version: String = line[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            .collect();
        if !version.is_empty() {
            return version;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn embedded_docs_have_nonempty_titles_and_declared_version() {
        let docs = &cached_agreements().documents;
        assert_eq!(docs.len(), 2);
        assert!(!docs[0].title.is_empty());
        assert!(!docs[1].title.is_empty());
        assert!(!docs[0].declared_version.is_empty());
        assert!(!docs[1].declared_version.is_empty());
    }

    /// 换行风格是构建环境决定的（Windows checkout 可能转 CRLF），不该影响同意键。
    #[test]
    fn version_key_and_content_hash_ignore_line_ending_style() {
        let docs = &cached_agreements().documents;
        let crlf: Vec<DesktopAgreementDoc> = docs
            .iter()
            .map(|doc| DesktopAgreementDoc {
                text: normalize_text(&to_crlf(&doc.text)),
                ..doc.clone()
            })
            .collect();
        assert_eq!(compute_version_key(&crlf), current_version());
        assert_eq!(compute_content_hash(&crlf), compute_content_hash(docs));
        assert!(!docs.iter().any(|doc| doc.text.contains('\r')));
    }

    #[test]
    fn version_key_tracks_declared_versions_only() {
        let docs = &cached_agreements().documents;
        let key = current_version();
        assert_eq!(key, compute_version_key(docs));
        for doc in docs {
            assert!(key.contains(&format!("{}@{}", doc.id, doc.declared_version)));
        }

        let mut edited = docs.clone();
        edited[0].text.push_str("\n\n<!-- 错别字修订 -->\n");
        assert_eq!(compute_version_key(&edited), key);
        edited[0].declared_version = "9.9".to_string();
        assert_ne!(compute_version_key(&edited), key);
    }

    /// 3.0.x 之前的记录以 content-hash 为键，升级后不能再弹一次门禁。
    #[test]
    fn legacy_content_hash_record_is_still_accepted() {
        let cache = cached_agreements();
        for legacy in &cache.legacy_versions {
            let dir = tempdir().unwrap();
            write_raw_record(dir.path(), legacy);
            assert!(!is_consent_required(dir.path()), "legacy key {legacy}");
            assert!(!load_payload(dir.path()).consent_required);
        }
    }

    #[test]
    fn unrelated_stored_version_still_requires_consent() {
        let dir = tempdir().unwrap();
        write_raw_record(dir.path(), "eula@0.9+privacy@0.9");
        assert!(is_consent_required(dir.path()));
    }

    fn write_raw_record(data_root: &Path, version: &str) {
        let config_dir = data_root.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join(CONSENT_FILE),
            format!(r#"{{"version":"{version}","acceptedAt":"2026-07-16T12:00:00.000Z"}}"#),
        )
        .unwrap();
    }

    #[test]
    fn parse_declared_version_reads_bilingual_header() {
        let text = "# Title\n\n- **版本 / Version:** 1.3\n\nbody";
        assert_eq!(parse_declared_version(text), "1.3");
    }

    #[test]
    fn load_payload_requires_consent_when_missing_record() {
        let dir = tempdir().unwrap();
        let payload = load_payload(dir.path());
        assert!(payload.consent_required);
        assert!(payload.accepted_at.is_none());
        assert_eq!(payload.version, current_version());
    }

    #[test]
    fn record_consent_clears_required_flag() {
        let dir = tempdir().unwrap();
        let version = current_version().to_string();
        record_consent(dir.path(), &version).unwrap();
        let after = load_payload(dir.path());
        assert!(!after.consent_required);
        assert!(after.accepted_at.is_some());
        assert_eq!(after.version, version);
    }

    #[test]
    fn record_consent_overwrites_existing_file_on_windows_path() {
        let dir = tempdir().unwrap();
        let version = current_version().to_string();
        record_consent(dir.path(), &version).unwrap();
        record_consent(dir.path(), &version).unwrap();
        assert!(!is_consent_required(dir.path()));
    }

    #[test]
    fn record_consent_rejects_stale_version() {
        let dir = tempdir().unwrap();
        let err = record_consent(dir.path(), "eula@0.1+privacy@0.1").unwrap_err();
        assert!(matches!(err, DesktopConsentError::VersionMismatch { .. }));
        let msg = err.to_command_string();
        assert!(msg.starts_with(DESKTOP_CONSENT_VERSION_MISMATCH_PREFIX));
    }

    #[test]
    fn ensure_accepted_errors_until_recorded() {
        let dir = tempdir().unwrap();
        let err = ensure_accepted(dir.path()).unwrap_err();
        assert!(matches!(err, DesktopConsentError::ConsentRequired { .. }));
        assert!(
            err.to_command_string()
                .starts_with(DESKTOP_CONSENT_REQUIRED_PREFIX)
        );

        record_consent(dir.path(), current_version()).unwrap();
        ensure_accepted(dir.path()).unwrap();
    }
}
