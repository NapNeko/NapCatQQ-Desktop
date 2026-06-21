//! Resume snapshot:自更新前持久化的 bot/daemon 状态,新版启动时还原
//!
//! 由 [UpdateOrchestrator::resume_after_update](crate::UpdateOrchestrator::resume_after_update)
//! 读取消费

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::error::UpdateError;

/// 自更新 resume snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateResumePoint {
    /// 协议版本(envelope,与前端契约同步)
    #[serde(default = "default_v")]
    pub v: u32,
    /// 升级前 desktop 版本
    pub from_version: String,
    /// 升级目标版本
    pub to_version: String,
    /// 升级时间戳
    pub initiated_at: DateTime<Utc>,
    /// 升级前在跑的 bot id 列表(用于新版启动时自动重启)
    pub running_bots: Vec<String>,
    /// 升级前 SnowLuma daemon 是否在跑
    pub snowluma_daemon_running: bool,
    /// 自定义注释
    pub note: Option<String>,
}

fn default_v() -> u32 {
    1
}

impl UpdateResumePoint {
    pub fn new(from_version: impl Into<String>, to_version: impl Into<String>) -> Self {
        Self {
            v: 1,
            from_version: from_version.into(),
            to_version: to_version.into(),
            initiated_at: Utc::now(),
            running_bots: Vec::new(),
            snowluma_daemon_running: false,
            note: None,
        }
    }

    pub fn with_running_bots(mut self, bots: Vec<String>) -> Self {
        self.running_bots = bots;
        self
    }

    pub fn with_snowluma_daemon(mut self, running: bool) -> Self {
        self.snowluma_daemon_running = running;
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Resume 持久化抽象
pub struct ResumeStore {
    snapshot_path: PathBuf,
}

impl ResumeStore {
    /// 默认路径:<data_root>/update-resume.json
    pub fn new(data_root: &std::path::Path) -> Self {
        Self {
            snapshot_path: data_root.join("update-resume.json"),
        }
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            snapshot_path: path.into(),
        }
    }

    /// 保存 resume snapshot
    pub async fn save(&self, point: &UpdateResumePoint) -> Result<(), UpdateError> {
        let bytes = serde_json::to_vec_pretty(point)?;
        if let Some(parent) = self.snapshot_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }
        fs::write(&self.snapshot_path, bytes).await?;
        Ok(())
    }

    /// 读取 resume snapshot文件不存在返回 Ok(None)
    pub async fn load(&self) -> Result<Option<UpdateResumePoint>, UpdateError> {
        match fs::read(&self.snapshot_path).await {
            Ok(bytes) => {
                let point: UpdateResumePoint = serde_json::from_slice(&bytes)?;
                Ok(Some(point))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(UpdateError::Io(e)),
        }
    }

    /// 清理 resume snapshot(在 resume_after_update 完成后调用)
    pub async fn clear(&self) -> Result<(), UpdateError> {
        match fs::remove_file(&self.snapshot_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(UpdateError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resume_point_serializes_with_v_field() {
        let p = UpdateResumePoint::new("0.1.0", "0.2.0");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"v\":1"));
        assert!(json.contains("\"from_version\":\"0.1.0\""));
    }

    #[tokio::test]
    async fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let store = ResumeStore::new(dir.path());
        let point = UpdateResumePoint::new("0.1.0", "0.2.0")
            .with_running_bots(vec!["100001".into(), "100002".into()])
            .with_snowluma_daemon(true);
        store.save(&point).await.unwrap();
        let loaded = store.load().await.unwrap().unwrap();
        assert_eq!(loaded.from_version, "0.1.0");
        assert_eq!(loaded.to_version, "0.2.0");
        assert_eq!(loaded.running_bots.len(), 2);
        assert!(loaded.snowluma_daemon_running);
    }

    #[tokio::test]
    async fn load_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        let store = ResumeStore::new(dir.path());
        let loaded = store.load().await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn clear_removes_existing_snapshot() {
        let dir = tempdir().unwrap();
        let store = ResumeStore::new(dir.path());
        store.save(&UpdateResumePoint::new("a", "b")).await.unwrap();
        store.clear().await.unwrap();
        assert!(store.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn clear_idempotent_when_missing() {
        let dir = tempdir().unwrap();
        let store = ResumeStore::new(dir.path());
        // 文件不存在时 clear 不应该报错
        store.clear().await.unwrap();
    }

    #[tokio::test]
    async fn save_creates_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir/deeper/snapshot.json");
        let store = ResumeStore::with_path(&path);
        store.save(&UpdateResumePoint::new("a", "b")).await.unwrap();
        assert!(path.exists());
    }
}
