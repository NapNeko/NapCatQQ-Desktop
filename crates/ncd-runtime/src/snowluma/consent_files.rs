use std::fs;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use ncd_backend_snowluma::{AgreementDoc, AgreementsPayload};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub(crate) enum SnowLumaConsentFileError {
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
    #[error("agreement version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    #[error("serialize consent record: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct ConsentRecord {
    version: String,
    #[serde(rename = "acceptedAt")]
    _accepted_at: String,
}

#[derive(Debug, Serialize)]
struct NewConsentRecord<'a> {
    version: &'a str,
    #[serde(rename = "acceptedAt")]
    accepted_at: String,
}

pub(crate) fn load_payload_from_runtime_root(
    runtime_root: &Path,
) -> Result<Option<AgreementsPayload>, SnowLumaConsentFileError> {
    let eula = read_agreement_file(runtime_root, "EULA.md")?;
    let privacy = read_agreement_file(runtime_root, "PRIVACY.md")?;
    let consent_version = read_consent_version(runtime_root);
    Ok(payload_from_agreement_texts(
        &eula,
        &privacy,
        consent_version.as_deref(),
    ))
}

/// 本机路径与远端 SSH 读到的正文共用：版本哈希、是否需要弹窗。
pub(crate) fn payload_from_agreement_texts(
    eula: &str,
    privacy: &str,
    consent_version: Option<&str>,
) -> Option<AgreementsPayload> {
    let docs = vec![
        agreement_doc("eula", eula),
        agreement_doc("privacy", privacy),
    ];
    if docs.iter().all(|doc| doc.text.trim().is_empty()) {
        return None;
    }
    let version = compute_agreements_version(&docs);
    let consent_required = consent_version != Some(version.as_str());
    Some(AgreementsPayload {
        version,
        consent_required,
        documents: docs,
    })
}

pub(crate) fn parse_consent_version_json(text: &str) -> Option<String> {
    let record: ConsentRecord = serde_json::from_str(text).ok()?;
    let trimmed = record.version.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) fn consent_record_json(version: &str) -> Result<Vec<u8>, SnowLumaConsentFileError> {
    let record = NewConsentRecord {
        version,
        accepted_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    Ok(serde_json::to_vec_pretty(&record)?)
}

pub(crate) fn record_consent_to_runtime_root(
    runtime_root: &Path,
    version: &str,
) -> Result<(), SnowLumaConsentFileError> {
    if let Some(payload) = load_payload_from_runtime_root(runtime_root)?
        && payload.version != version
    {
        return Err(SnowLumaConsentFileError::VersionMismatch {
            expected: payload.version,
            actual: version.to_string(),
        });
    }

    let config_dir = runtime_root.join("config");
    fs::create_dir_all(&config_dir).map_err(|source| SnowLumaConsentFileError::CreateDir {
        path: config_dir.clone(),
        source,
    })?;

    let path = config_dir.join("consent.json");
    let tmp = config_dir.join("consent.json.tmp");
    let bytes = consent_record_json(version)?;
    fs::write(&tmp, bytes).map_err(|source| SnowLumaConsentFileError::Write {
        path: tmp.clone(),
        source,
    })?;
    replace_file(&tmp, &path)?;
    Ok(())
}

fn agreement_doc(id: &str, text: &str) -> AgreementDoc {
    let meta = parse_agreement_meta(text);
    AgreementDoc {
        id: id.to_string(),
        title: meta.title,
        declared_version: meta.declared_version,
        text: text.to_string(),
    }
}

fn read_agreement_file(
    runtime_root: &Path,
    file_name: &str,
) -> Result<String, SnowLumaConsentFileError> {
    let candidates = [
        runtime_root.join(file_name),
        runtime_root.join("dist").join(file_name),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(fs::read_to_string(&candidate).unwrap_or_default());
        }
    }
    Ok(String::new())
}

fn replace_file(from: &Path, to: &Path) -> Result<(), SnowLumaConsentFileError> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(to).map_err(|source| SnowLumaConsentFileError::Remove {
                path: to.to_path_buf(),
                source,
            })?;
            fs::rename(from, to).map_err(|source| SnowLumaConsentFileError::Rename {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                source,
            })
        }
        Err(source) => Err(SnowLumaConsentFileError::Rename {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            source,
        }),
    }
}

fn read_consent_version(runtime_root: &Path) -> Option<String> {
    let path = runtime_root.join("config").join("consent.json");
    let text = fs::read_to_string(path).ok()?;
    parse_consent_version_json(&text)
}

fn compute_agreements_version(docs: &[AgreementDoc]) -> String {
    let mut hash = Sha256::new();
    for doc in docs {
        hash.update(doc.id.as_bytes());
        hash.update(b"\0");
        hash.update(doc.text.as_bytes());
        hash.update(b"\0");
    }
    hex::encode(hash.finalize())[..16].to_string()
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

struct AgreementMeta {
    title: String,
    declared_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn computes_upstream_compatible_version_and_detects_consent() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("EULA.md"),
            "# EULA\n\n- **版本 / Version:** 1.0\n\nterms",
        )
        .unwrap();
        fs::write(dir.path().join("PRIVACY.md"), "# Privacy\n\nprivacy").unwrap();

        let payload = load_payload_from_runtime_root(dir.path()).unwrap().unwrap();
        assert!(payload.consent_required);
        assert_eq!(payload.documents[0].title, "EULA");
        assert_eq!(payload.documents[0].declared_version, "1.0");

        record_consent_to_runtime_root(dir.path(), &payload.version).unwrap();
        let after = load_payload_from_runtime_root(dir.path()).unwrap().unwrap();
        assert!(!after.consent_required);
    }

    #[test]
    fn missing_documents_do_not_claim_consent_is_required() {
        let dir = tempdir().unwrap();
        assert!(
            load_payload_from_runtime_root(dir.path())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_stale_version_on_write() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("EULA.md"), "# EULA\n\nterms").unwrap();
        fs::write(dir.path().join("PRIVACY.md"), "# Privacy\n\nprivacy").unwrap();

        let err = record_consent_to_runtime_root(dir.path(), "stale")
            .expect_err("stale version must be rejected");

        assert!(matches!(
            err,
            SnowLumaConsentFileError::VersionMismatch { .. }
        ));
    }

    #[test]
    fn payload_from_texts_matches_file_loader() {
        let eula = "# EULA\n\n- **版本 / Version:** 1.0\n\nterms";
        let privacy = "# Privacy\n\nprivacy";
        let from_texts = payload_from_agreement_texts(eula, privacy, None).unwrap();
        assert!(from_texts.consent_required);

        let recorded = parse_consent_version_json(
            std::str::from_utf8(&consent_record_json(&from_texts.version).unwrap()).unwrap(),
        );
        let after = payload_from_agreement_texts(eula, privacy, recorded.as_deref()).unwrap();
        assert!(!after.consent_required);
        assert_eq!(after.version, from_texts.version);
    }

    #[test]
    fn empty_texts_do_not_require_consent() {
        assert!(payload_from_agreement_texts("", "  ", None).is_none());
    }
}
