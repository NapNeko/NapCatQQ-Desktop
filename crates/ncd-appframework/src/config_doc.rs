//! 配置文件原文读写、格式校验与版本检查。

use crate::adapter::apply_with_backup_snapshots;
use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue, AppConfigText};
use ncd_host::{Host, HostError, HostPath};
use ncd_traits::AppFrameworkError;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MISSING_REVISION: &str = "missing";

/// 校验结果的便捷收集器
#[derive(Debug, Default)]
pub struct IssueSink {
    issues: Vec<AppConfigIssue>,
}

impl IssueSink {
    pub fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(AppConfigIssue::new(path, message));
    }

    pub fn extend(&mut self, extra: Vec<AppConfigIssue>) {
        self.issues.extend(extra);
    }

    pub fn into_vec(self) -> Vec<AppConfigIssue> {
        self.issues
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// 闭区间；两头都可以不限
    pub fn range(&mut self, path: &str, value: f64, min: Option<f64>, max: Option<f64>) {
        let fmt = |v: f64| {
            if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                v.to_string()
            }
        };
        match (min, max) {
            (Some(lo), Some(hi)) if !(lo..=hi).contains(&value) => {
                self.push(path, format!("要在 {} 到 {} 之间", fmt(lo), fmt(hi)));
            }
            (Some(lo), None) if value < lo => self.push(path, format!("不能小于 {}", fmt(lo))),
            (None, Some(hi)) if value > hi => self.push(path, format!("不能大于 {}", fmt(hi))),
            _ => {}
        }
    }

    pub fn one_of(&mut self, path: &str, value: &str, options: &[&str]) {
        if !options.contains(&value) {
            self.push(
                path,
                format!("只能是 {} 之一，现在是 {value:?}", options.join(" / ")),
            );
        }
    }
}

pub fn revision_of(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex = format!("{digest:x}");
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

pub async fn read_document(
    host: &dyn Host,
    install_dir: &HostPath,
    doc: &AppConfigDocument,
) -> Result<DocumentSnapshot, AppFrameworkError> {
    let path = document_path(install_dir, doc);
    let bytes = match host.read_file(&path).await {
        Ok(bytes) => bytes,
        Err(HostError::PathNotFound { .. }) => {
            return Ok(DocumentSnapshot {
                doc: doc.clone(),
                text: None,
                revision: MISSING_REVISION.to_string(),
            });
        }
        Err(error) => return Err(AppFrameworkError::Host(error.to_string())),
    };
    Ok(DocumentSnapshot {
        doc: doc.clone(),
        revision: revision_of(&bytes),
        text: Some(String::from_utf8(bytes.to_vec()).map_err(|_| {
            AppFrameworkError::Validation(format!("{} 不是 UTF-8 文本，无法编辑", doc.rel_path))
        })?),
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

/// 原始文本写前预检：JSON / TOML / YAML 必须能解析；dotenv 不限。
pub fn validate_text(format: AppConfigFormat, text: &str) -> Result<(), AppFrameworkError> {
    // AstrBot 会写入 BOM；这里只处理校验视图，保存仍保留用户原文。
    let text = text.trim_start_matches('\u{feff}');
    let err = match format {
        AppConfigFormat::Json => serde_json::from_str::<serde_json::Value>(text)
            .err()
            .map(|e| format!("JSON 语法错误: {e}")),
        AppConfigFormat::Toml => toml::from_str::<toml::Value>(text)
            .err()
            .map(|e| format!("TOML 语法错误: {e}")),
        AppConfigFormat::Yaml => serde_yaml::from_str::<serde_yaml::Value>(text)
            .err()
            .map(|e| format!("YAML 语法错误: {e}")),
        AppConfigFormat::DotEnv => None,
    };
    match err {
        Some(message) => Err(AppFrameworkError::ConfigInvalid(vec![AppConfigIssue::new(
            "text", message,
        )])),
        None => Ok(()),
    }
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

pub async fn write_document_text(
    host: &dyn Host,
    install_dir: &HostPath,
    doc: &AppConfigDocument,
    text: &str,
    base_revision: Option<&str>,
    write_sidecar: bool,
) -> Result<AppConfigText, AppFrameworkError> {
    validate_text(doc.format, text)?;
    let current = read_document(host, install_dir, doc).await?;
    if base_revision.is_some_and(|base| base != current.revision) {
        return Err(AppFrameworkError::ConfigConflict(doc.id.clone()));
    }
    let path = document_path(install_dir, doc);
    ensure_parent_dir(host, &path).await?;
    let originals = [(path.clone(), current.text.map(String::into_bytes))];
    apply_with_backup_snapshots(host, &originals, write_sidecar, || async {
        host.write_file(&path, text.as_bytes())
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))
    })
    .await?;
    Ok(AppConfigText {
        doc_id: doc.id.clone(),
        text: text.to_string(),
        revision: revision_of(text.as_bytes()),
    })
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

    #[test]
    fn revision_is_stable_and_short() {
        let a = revision_of(b"hello");
        assert_eq!(a.len(), 16);
        assert_eq!(a, revision_of(b"hello"));
        assert_ne!(a, revision_of(b"hello\n"));
    }

    #[test]
    fn validate_text_checks_structured_formats() {
        assert!(validate_text(AppConfigFormat::Json, "{\"a\":1}").is_ok());
        assert!(matches!(
            validate_text(AppConfigFormat::Json, "{oops"),
            Err(AppFrameworkError::ConfigInvalid(_))
        ));
        assert!(validate_text(AppConfigFormat::Json, "\u{feff}{oops").is_err());
        assert!(validate_text(AppConfigFormat::Toml, "[tool]\nname = \"x\"\n").is_ok());
        assert!(matches!(
            validate_text(AppConfigFormat::Toml, "= broken"),
            Err(AppFrameworkError::ConfigInvalid(_))
        ));
        assert!(validate_text(AppConfigFormat::DotEnv, "whatever = = =").is_ok());
        assert!(validate_text(AppConfigFormat::Yaml, "key: value\n").is_ok());
        assert!(validate_text(AppConfigFormat::Yaml, "key: [").is_err());
    }

    fn json_doc() -> AppConfigDocument {
        AppConfigDocument {
            id: "config".into(),
            label: "配置".into(),
            rel_path: "config/app.json".into(),
            format: AppConfigFormat::Json,
            hot_reload: false,
            reload_note: None,
        }
    }

    #[tokio::test]
    async fn raw_save_preserves_text_and_rejects_external_edits() {
        let host = crate::config_backup::tests::TestHost::new("remote:files");
        let root = HostPath::from_posix("/app");
        let doc = json_doc();
        host.put("/app/config/app.json", b"{}\n");
        let before = read_document(host.as_ref(), &root, &doc).await.unwrap();
        let text = "\u{feff}{\r\n  \"custom\": [1, 2]\r\n}\r\n";
        let saved = write_document_text(
            host.as_ref(),
            &root,
            &doc,
            text,
            Some(&before.revision),
            true,
        )
        .await
        .unwrap();
        assert_eq!(saved.text, text);
        assert_eq!(
            host.content("/app/config/app.json").unwrap(),
            text.as_bytes()
        );
        assert_eq!(
            host.content("/app/config/app.json.ncd.bak").unwrap(),
            b"{}\n"
        );
        host.put("/app/config/app.json", b"{\"external\":true}");
        assert!(matches!(
            write_document_text(
                host.as_ref(),
                &root,
                &doc,
                "{}",
                Some(&saved.revision),
                false
            )
            .await,
            Err(AppFrameworkError::ConfigConflict(_))
        ));
        assert_eq!(
            host.content("/app/config/app.json").unwrap(),
            b"{\"external\":true}"
        );
        write_document_text(host.as_ref(), &root, &doc, text, None, false)
            .await
            .unwrap();
        assert_eq!(
            host.content("/app/config/app.json").unwrap(),
            text.as_bytes()
        );
    }

    #[tokio::test]
    async fn permission_failure_is_not_treated_as_a_missing_config() {
        let host = crate::config_backup::tests::TestHost::new("remote:files");
        host.put("/app/config/app.json", b"{}");
        host.deny_read("/app/config/app.json");
        assert!(matches!(
            read_document(host.as_ref(), &HostPath::from_posix("/app"), &json_doc()).await,
            Err(AppFrameworkError::Host(_)),
        ));
    }

    #[tokio::test]
    async fn raw_save_reads_existing_file_once_for_version_and_backup() {
        for check_revision in [true, false] {
            let host = crate::config_backup::tests::TestHost::new("remote:files");
            let root = HostPath::from_posix("/app");
            let original = "\u{feff}{\"original\":true}\r\n";
            let revision = revision_of(original.as_bytes());
            host.put("/app/config/app.json", original.as_bytes());
            write_document_text(
                host.as_ref(),
                &root,
                &json_doc(),
                "{}",
                check_revision.then_some(revision.as_str()),
                true,
            )
            .await
            .unwrap();
            assert_eq!(host.access_counts("/app/config/app.json"), (0, 1));
            assert_eq!(
                host.content("/app/config/app.json.ncd.bak").unwrap(),
                original.as_bytes()
            );
        }
    }

    #[tokio::test]
    async fn raw_save_checks_syntax_and_rolls_back_partial_writes() {
        let host = crate::config_backup::tests::TestHost::new("remote:files");
        let root = HostPath::from_posix("/app");
        let doc = json_doc();
        let original = "\u{feff}{}\r\n";
        host.put("/app/config/app.json", original.as_bytes());
        assert!(matches!(
            write_document_text(host.as_ref(), &root, &doc, "{broken", None, false).await,
            Err(AppFrameworkError::ConfigInvalid(_))
        ));
        assert_eq!(
            host.content("/app/config/app.json").unwrap(),
            original.as_bytes()
        );
        host.fail_next_write("/app/config/app.json");
        assert!(
            write_document_text(host.as_ref(), &root, &doc, "{\"ok\":true}", None, false)
                .await
                .is_err()
        );
        assert_eq!(
            host.content("/app/config/app.json").unwrap(),
            original.as_bytes()
        );
        assert!(host.content("/app/config/app.json.ncd.bak").is_none());
    }

    #[tokio::test]
    async fn failed_raw_creation_removes_partial_file() {
        let host = crate::config_backup::tests::TestHost::new("remote:files");
        host.fail_next_write("/app/config/app.json");
        assert!(
            write_document_text(
                host.as_ref(),
                &HostPath::from_posix("/app"),
                &json_doc(),
                "{}",
                Some(MISSING_REVISION),
                true,
            )
            .await
            .is_err()
        );
        assert!(host.content("/app/config/app.json").is_none());
        assert!(host.content("/app/config/app.json.ncd.bak").is_none());
    }

    #[tokio::test]
    async fn missing_files_can_be_created_but_invalid_utf8_is_not_replaced() {
        let host = crate::config_backup::tests::TestHost::new("remote:files");
        let root = HostPath::from_posix("/app");
        let doc = json_doc();
        let missing = read_document(host.as_ref(), &root, &doc).await.unwrap();
        assert!(missing.text.is_none());
        write_document_text(
            host.as_ref(),
            &root,
            &doc,
            "{}",
            Some(&missing.revision),
            false,
        )
        .await
        .unwrap();
        host.put("/app/config/app.json", &[0xff]);
        assert!(read_document(host.as_ref(), &root, &doc).await.is_err());
        assert!(
            write_document_text(host.as_ref(), &root, &doc, "{}", None, false)
                .await
                .is_err()
        );
        assert_eq!(host.content("/app/config/app.json").unwrap(), [0xff]);
    }
}
