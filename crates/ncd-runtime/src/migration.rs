use std::path::Path;

use serde_json::Value;

use crate::app_config_migration::migrate_app_config;
use crate::bot_config_migration::migrate_bot_config;
use crate::errors::MigrationError;
use crate::legacy_discovery::{LegacyDiscovery, LegacySelection};
use crate::models::MigrationSource;
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
            Err(error) => MigrationReport::failed(error.to_string()).into(),
        }
    }

    pub fn run(&self) -> Result<MigrationReport, MigrationError> {
        if self.store.load_schema_version()? == crate::kinds::SchemaVersion::CURRENT {
            return Ok(MigrationReport::clean());
        }

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
            let app = migrate_app_config(self.read_source_json(app_path)?);
            rules.extend(app.rules_applied);
            tx = tx.write(self.store.config_dir().join("config.json"), app.payload);
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
