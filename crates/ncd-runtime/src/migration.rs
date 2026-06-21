use std::path::Path;

use serde_json::Value;
use tracing::{info, warn};

use crate::app_config_migration::migrate_app_config;
use crate::bot_config_migration::migrate_bot_config;
use crate::errors::MigrationError;
use crate::legacy_discovery::{LegacyDiscovery, LegacySelection};
use crate::models::{MigrationSource, MigrationWarning};
use crate::report::MigrationReport;
use crate::traits::{ConfigStore, JsonTransaction, PathProbe, SecretStore};

pub struct MigrationOrchestrator<'a> {
    store: &'a dyn ConfigStore,
    probe: &'a dyn PathProbe,
    secrets: &'a dyn SecretStore,
}

impl<'a> MigrationOrchestrator<'a> {
    pub fn new(
        store: &'a dyn ConfigStore,
        probe: &'a dyn PathProbe,
        secrets: &'a dyn SecretStore,
    ) -> Self {
        Self {
            store,
            probe,
            secrets,
        }
    }

    pub fn bootstrap(&self) -> crate::bootstrap::BootstrapSnapshot {
        match self.run() {
            Ok(report) => {
                let _ = self.store.save_migration_report(&report);
                report.into()
            }
            Err(error) => {
                // 失败也尽力把报告落盘:保留首次失败证据,重启后仍能查到原因否则
                // 失败信息只在内存,重启即丢,旧用户升级踩坑没有任何排障线索落盘
                // 本身再失败就只能放弃(best-effort),不掩盖原始迁移错误
                warn!(
                    target: "ncd_runtime::migration",
                    err = %error,
                    "启动迁移失败"
                );
                let report = MigrationReport::failed(error.to_string());
                let _ = self.store.save_migration_report(&report);
                report.into()
            }
        }
    }

    pub fn run(&self) -> Result<MigrationReport, MigrationError> {
        if self.store.load_schema_version()? == crate::kinds::SchemaVersion::CURRENT {
            info!(target: "ncd_runtime::migration", "schema current, skip migration");
            return Ok(MigrationReport::clean());
        }

        info!(target: "ncd_runtime::migration", "legacy migration run starting");
        let discovery = LegacyDiscovery::new(self.probe);
        let selections = discovery.discover()?;
        let Some(selection) = selections.into_iter().next() else {
            return self.initialize_empty_config();
        };

        self.migrate_selection(selection)
    }

    fn initialize_empty_config(&self) -> Result<MigrationReport, MigrationError> {
        let payload = serde_json::json!({
            "info": {"configVersion": crate::bot_config_migration::BOT_CONFIG_COMPAT_VERSION},
            "bots": [],
        });
        let tx = JsonTransaction::new().write(self.store.config_dir().join("bot.json"), payload);
        let tx_report = self.store.apply_transaction(tx)?;
        let mut report =
            MigrationReport::migrated(vec!["initialized empty bot config".to_string()]);
        if let Some(backup) = tx_report.backup {
            report = report.with_backup(backup);
        }
        Ok(report)
    }

    fn migrate_selection(
        &self,
        selection: LegacySelection,
    ) -> Result<MigrationReport, MigrationError> {
        let mut tx = JsonTransaction::new();
        let mut rules = Vec::new();
        let mut warnings = selection.warnings.clone();

        if let Some(app_path) = &selection.app_config {
            let raw = self.read_source_json(app_path)?;
            if crate::app_config_migration::looks_like_app_config(&raw) {
                let app = migrate_app_config(raw);
                rules.extend(app.rules_applied);
                tx = tx.write(self.store.config_dir().join("config.json"), app.payload);
            } else {
                // 误选的无关 / 非对象 config.json:跳过不写,留 warning绝不强转空对象
                // 写 Info.ConfigVersion 当"成功迁移",否则垃圾文件会污染生产配置根
                warnings.push(MigrationWarning::new(
                    "app_config_skipped",
                    format!(
                        "跳过不像应用配置的 {}(非对象或缺少已知配置段)",
                        app_path.display()
                    ),
                ));
            }
        }

        if let Some(bot_path) = &selection.bot_config {
            let bot = migrate_bot_config(self.read_source_json(bot_path)?, self.secrets)?;
            rules.extend(bot.rules_applied);
            warnings.extend(bot.warnings);
            tx = tx.write(self.store.config_dir().join("bot.json"), bot.payload);
            for summary in bot.summaries {
                let payload = serde_json::to_value(&summary)
                    .map_err(|error| MigrationError::InvalidPayload(error.to_string()))?;
                tx = tx.write(
                    self.store
                        .config_dir()
                        .join("runtime-summary")
                        .join(format!("{}.json", summary.bot_id.as_str())),
                    payload,
                );
            }
        }

        if tx.is_empty() {
            return Ok(MigrationReport::clean());
        }

        let tx_report = self.store.apply_transaction(tx)?;
        let source = MigrationSource {
            root: selection.root,
            app_config: selection.app_config,
            bot_config: selection.bot_config,
            auxiliary_files: selection.auxiliary_files,
        };
        let mut report = MigrationReport::migrated(rules)
            .with_source(source)
            .with_warnings(warnings);
        if let Some(backup) = tx_report.backup {
            report = report.with_backup(backup);
        }
        info!(
            target: "ncd_runtime::migration",
            rules = report.rules_applied.len(),
            warnings = report.warnings.len(),
            "旧版配置迁移完成"
        );
        Ok(report)
    }

    fn read_source_json(&self, path: &Path) -> Result<Value, MigrationError> {
        if !self.probe.is_allowed(path) {
            return Err(MigrationError::InvalidPayload(format!(
                "legacy config path is outside allowed roots: {}",
                path.display()
            )));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| MigrationError::InvalidPayload(error.to_string()))?;
        serde_json::from_str(&text)
            .map_err(|error| MigrationError::InvalidPayload(error.to_string()))
    }
}

pub fn migrate_payload_for_tests(
    payload: Value,
    secrets: &dyn SecretStore,
) -> Result<Value, MigrationError> {
    Ok(migrate_bot_config(payload, secrets)?.payload)
}
