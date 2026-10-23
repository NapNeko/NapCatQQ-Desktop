use super::*;
use ncd_traits::ConfigStore;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct AppMove {
    pub source: AppInstance,
    pub target: AppInstance,
    pub adopt: Option<adopt::AdoptSnapshot>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct MigrationJournal {
    pub job: BotMigrationJob,
    pub source: BotConfig,
    pub target: BotConfig,
    pub apps: Vec<AppMove>,
    pub source_was_running: bool,
    pub source_touched: bool,
    pub config_switched: bool,
}

#[derive(Default)]
pub(crate) struct MigrationState {
    pub gate: Arc<tokio::sync::Mutex<()>>,
    pub active: std::sync::Mutex<Option<String>>,
}

impl AppManager {
    pub(super) fn migration_journal_dir(&self) -> PathBuf {
        self.data_root.join("config").join("bot-migrations")
    }

    pub(super) async fn migration_save(&self, journal: &MigrationJournal) -> Result<(), String> {
        let payload = serde_json::to_value(journal).map_err(|e| e.to_string())?;
        let path = self
            .migration_journal_dir()
            .join(format!("{}.json", journal.job.id));
        let root = self.data_root.clone();
        tokio::task::spawn_blocking(move || {
            crate::LocalConfigStore::new(root)
                .write_json_atomic(&path, &payload)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub(super) async fn migration_load(&self, id: &str) -> Result<MigrationJournal, String> {
        uuid::Uuid::parse_str(id).map_err(|_| "迁移任务标识不合法")?;
        let path = self.migration_journal_dir().join(format!("{id}.json"));
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| format!("读取迁移记录失败：{e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("迁移记录损坏：{e}"))
    }

    pub(super) async fn migration_journals(&self) -> Result<Vec<MigrationJournal>, String> {
        let mut entries = match tokio::fs::read_dir(self.migration_journal_dir()).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        let mut journals = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("迁移记录路径不合法")?;
            journals.push(self.migration_load(id).await?);
        }
        Ok(journals)
    }

    pub(super) async fn migration_phase(
        &self,
        journal: &mut MigrationJournal,
        phase: BotMigrationPhase,
        message: &str,
    ) -> Result<(), String> {
        journal.job.phase = phase;
        journal.job.message = message.into();
        journal.job.updated_at_ms = now_ms();
        self.migration_save(journal).await
    }
}
