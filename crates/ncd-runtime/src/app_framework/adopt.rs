//! 导入快照落在 `data_root/config/app-adopts/{id}.json`，不进用户项目。

use std::path::{Path, PathBuf};

use ncd_appframework::{AdoptedFile, capture_adopted_files};
use ncd_domain::AppInstanceId;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};

const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptSnapshot {
    #[serde(default)]
    pub version: u32,
    pub instance_id: String,
    pub host_id: String,
    pub install_dir: String,
    pub captured_at_ms: u64,
    #[serde(default)]
    pub files: Vec<AdoptedFile>,
    #[serde(default)]
    pub supervisors: Vec<String>,
}

pub struct AdoptStore {
    dir: PathBuf,
}

impl AdoptStore {
    pub fn new(data_root: &Path) -> Self {
        Self {
            dir: data_root.join("config").join("app-adopts"),
        }
    }

    fn path(&self, id: &AppInstanceId) -> PathBuf {
        self.dir.join(format!("{}.json", id.as_str()))
    }

    pub fn load(&self, id: &AppInstanceId) -> Result<Option<AdoptSnapshot>, AppFrameworkError> {
        let path = self.path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| AppFrameworkError::Store(format!("读导入快照失败: {e}")))?;
        let snap = serde_json::from_slice(&bytes)
            .map_err(|e| AppFrameworkError::Store(format!("解析导入快照失败: {e}")))?;
        Ok(Some(snap))
    }

    pub fn save(&self, snap: &AdoptSnapshot) -> Result<(), AppFrameworkError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| AppFrameworkError::Store(format!("创建导入快照目录失败: {e}")))?;
        let bytes = serde_json::to_vec_pretty(snap)
            .map_err(|e| AppFrameworkError::Store(e.to_string()))?;
        let path = self.path(&AppInstanceId::new(&snap.instance_id));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)
            .map_err(|e| AppFrameworkError::Store(format!("写导入快照失败: {e}")))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| AppFrameworkError::Store(format!("落盘导入快照失败: {e}")))?;
        Ok(())
    }

    pub fn remove(&self, id: &AppInstanceId) -> Result<(), AppFrameworkError> {
        let path = self.path(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| AppFrameworkError::Store(format!("删除导入快照失败: {e}")))?;
        }
        Ok(())
    }
}

pub async fn capture_snapshot(
    host: &dyn Host,
    instance_id: &AppInstanceId,
    host_id: &str,
    install_dir: &str,
    rels: &[String],
    supervisors: &[String],
    captured_at_ms: u64,
) -> Result<AdoptSnapshot, AppFrameworkError> {
    let root = HostPath::from_posix(install_dir);
    let files = capture_adopted_files(host, &root, rels).await?;
    Ok(AdoptSnapshot {
        version: FILE_VERSION,
        instance_id: instance_id.as_str().to_string(),
        host_id: host_id.to_string(),
        install_dir: install_dir.to_string(),
        captured_at_ms,
        files,
        supervisors: supervisors.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AdoptStore::new(tmp.path());
        let id = AppInstanceId::new("abcd1234");
        let snap = AdoptSnapshot {
            version: FILE_VERSION,
            instance_id: id.as_str().to_string(),
            host_id: "local".into(),
            install_dir: "/root/bot".into(),
            captured_at_ms: 9,
            files: vec![AdoptedFile {
                rel_path: ".env".into(),
                existed: true,
                text: Some("PORT=1\n".into()),
                skip_reason: None,
            }],
            supervisors: vec!["bot-xiuxian".into()],
        };
        store.save(&snap).unwrap();
        let loaded = store.load(&id).unwrap().expect("snapshot");
        assert_eq!(loaded.files[0].text.as_deref(), Some("PORT=1\n"));
        assert_eq!(loaded.supervisors, vec!["bot-xiuxian".to_string()]);
        store.remove(&id).unwrap();
        assert!(store.load(&id).unwrap().is_none());
    }
}
