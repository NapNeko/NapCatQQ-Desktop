//! 诊断会话的有界档案与原子写入。

use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use ncd_domain::assistant::{AssistantSession, AssistantSessionState, AssistantSettings};
use ncd_traits::ConfigStore;
use serde::{Deserialize, Serialize};

const ARCHIVE_LIMIT: u64 = 1024 * 1024;
pub(super) const SESSION_LIMIT: usize = 50;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredSession {
    pub v: u32,
    pub session: AssistantSession,
    pub remote_session_id: Option<String>,
    #[serde(default)]
    pub builtin_history: super::builtin::History,
}

#[derive(Clone)]
pub(super) struct AssistantStorage {
    root: PathBuf,
    writer: crate::LocalConfigStore,
}

impl AssistantStorage {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            writer: crate::LocalConfigStore::new(root),
        }
    }

    pub fn workspace(&self, id: &str) -> Result<PathBuf, String> {
        checked_id(id)?;
        Ok(self.root.join("runtime/assistant").join(id))
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        checked_id(id)?;
        Ok(self
            .root
            .join("state/assistant/sessions")
            .join(format!("{id}.json")))
    }

    pub fn settings(&self) -> Result<AssistantSettings, String> {
        read_json(&self.root.join("config/assistant.json")).map(|value| value.unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &AssistantSettings) -> Result<(), String> {
        self.write(&self.root.join("config/assistant.json"), settings)
    }

    pub fn load(&self) -> (Vec<StoredSession>, Vec<String>) {
        let directory = self.root.join("state/assistant/sessions");
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return (Vec::new(), Vec::new()),
            Err(_) => {
                return (
                    Vec::new(),
                    vec!["无法读取诊断会话目录，已有档案未改动".into()],
                );
            }
        };
        let mut sessions = Vec::new();
        let mut notices = Vec::new();
        let candidates = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_owned();
                if checked_id(&id).is_err() {
                    return None;
                }
                Some((path, id))
            })
            .take(SESSION_LIMIT + 1);
        for (index, (path, id)) in candidates.enumerate() {
            if index >= SESSION_LIMIT {
                notices.push("会话档案超过50个，请删除不再需要的会话".into());
                break;
            }
            let loaded: Result<Option<StoredSession>, _> = read_json(&path);
            match loaded {
                Ok(Some(mut stored)) if stored.v == 1 && stored.session.info.id == id => {
                    stored.session.permissions.clear();
                    stored.session.questions.clear();
                    if stored.session.info.state != AssistantSessionState::Closed {
                        stored.session.info.state = AssistantSessionState::Disconnected;
                    }
                    sessions.push(stored);
                }
                _ => notices.push(format!("会话 {id} 的档案无法读取，原文件已保留")),
            }
        }
        (sessions, notices)
    }

    pub fn save(&self, stored: &StoredSession) -> Result<(), String> {
        self.write(&self.path(&stored.session.info.id)?, stored)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        match std::fs::remove_file(self.path(id)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(_) => Err("删除诊断会话档案失败".into()),
        }
    }

    fn write(&self, path: &Path, value: &impl Serialize) -> Result<(), String> {
        let json = serde_json::to_value(value).map_err(|_| "序列化诊断档案失败")?;
        let bytes = serde_json::to_vec(&json).map_err(|_| "序列化诊断档案失败")?;
        if bytes.len() as u64 > ARCHIVE_LIMIT {
            return Err("诊断会话档案超过大小限制".into());
        }
        self.writer
            .write_json_atomic(path, &json)
            .map_err(|_| "保存诊断档案失败".into())
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("读取诊断档案失败".into()),
    };
    let mut bytes = Vec::new();
    file.take(ARCHIVE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取诊断档案失败")?;
    if bytes.len() as u64 > ARCHIVE_LIMIT {
        return Err("诊断档案超过大小限制，原文件已保留".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "诊断档案损坏，原文件已保留".into())
}

pub(super) fn checked_id(id: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        Ok(())
    } else {
        Err("诊断会话编号无效".into())
    }
}

pub(super) async fn persist_session(
    storage: &AssistantStorage,
    session: &super::session::Session,
) -> Result<(), String> {
    let _persist = session.persisting.lock().await;
    if session.deleted.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    let storage = storage.clone();
    let mut stored = session.stored();
    stored.session.permissions.clear();
    stored.session.questions.clear();
    tokio::task::spawn_blocking(move || storage.save(&stored))
        .await
        .map_err(|_| "保存诊断会话失败")?
}
