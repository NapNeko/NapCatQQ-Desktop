//! UpdateOrchestrator: 自更新流水线编排器
//!
//! 五个核心操作: check / precheck / install_with_graceful_shutdown /
//! resume_after_update / record_failure / detect_pending_failures
//!
//! TODO: install_with_graceful_shutdown 目前只保存 resume snapshot
//! 然后调 provider.download_and_install; 等 BotManager 重构完成后,
//! 在此处加"先 graceful stop 在跑 bot / SnowLuma daemon 再安装"

use std::path::PathBuf;
use std::sync::Arc;

use ncd_domain::SchemaVersion;
use semver::Version;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::channel::UpdateChannel;
use crate::error::UpdateError;
use crate::provider::UpdateProvider;
use crate::resume::{ResumeStore, UpdateResumePoint};
use crate::types::{AvailableUpdate, PrecheckReport, RecordedFailure, UpdatePhase};

pub struct UpdateOrchestrator {
    provider: Arc<dyn UpdateProvider>,
    resume_store: ResumeStore,
    /// failure 日志路径: data_root/update-failures.jsonl
    failures_path: PathBuf,
    current_schema: SchemaVersion,
    current_version: Version,
}

impl UpdateOrchestrator {
    /// - provider: 更新源(tauri-plugin-updater wrapper / Mock)
    /// - data_root: 数据根目录(<data_root>/update-resume.json + update-failures.jsonl)
    pub fn new(
        provider: Arc<dyn UpdateProvider>,
        data_root: &std::path::Path,
        current_version: Version,
        current_schema: SchemaVersion,
    ) -> Self {
        Self {
            provider,
            resume_store: ResumeStore::new(data_root),
            failures_path: data_root.join("update-failures.jsonl"),
            current_schema,
            current_version,
        }
    }

    // ===== 1. check =====

    /// 向 provider 请求更新; 过滤掉"伪更新"(版本 <= 当前版本)
    pub async fn check(
        &self,
        channel: UpdateChannel,
    ) -> Result<Option<AvailableUpdate>, UpdateError> {
        info!(target: "ncd_update", ?channel, "check for updates");
        match self.provider.check(channel).await {
            Ok(Some(u)) => {
                // server 可能返回相同或更低版本, 直接过滤
                if u.version <= self.current_version {
                    return Ok(None);
                }
                Ok(Some(u))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ===== 2. precheck =====

    /// schema 兼容预检
    pub async fn precheck(&self, update: &AvailableUpdate) -> Result<PrecheckReport, UpdateError> {
        let current = self.current_schema;
        let target = update.schema_version;

        if current == target {
            return Ok(PrecheckReport::ok());
        }

        // 不允许降级 schema
        if target.get() < current.get() {
            return Ok(PrecheckReport::blocked(format!(
                "target schema {target:?} is lower than current {current:?}: cannot downgrade",
            )));
        }

        // 跨度 >= 3: 太大, 用户应该逐版本升级
        let gap = target.get().saturating_sub(current.get());
        if gap >= 3 {
            return Ok(PrecheckReport::ok().add_blocking(format!(
                "schema gap too large: {current:?} to {target:?} (gap={gap}); upgrade incrementally"
            )));
        }

        // 跨度 1-2: 数据需要迁移但可行
        let mut report = PrecheckReport::ok().add_warning(format!(
            "schema upgrade from {current:?} to {target:?}: data will be migrated automatically"
        ));
        // 经验估算: 每 schema gap 200ms
        report.estimated_migration_time_ms = gap as u64 * 200;
        Ok(report)
    }

    // ===== 3. install_with_graceful_shutdown =====

    /// TODO: 等 BotManager 重构完成, 在此处加 graceful stop 在跑 bot / SnowLuma daemon
    pub async fn install_with_graceful_shutdown(
        &self,
        update: AvailableUpdate,
        running_bots: Vec<String>,
        snowluma_daemon_running: bool,
    ) -> Result<(), UpdateError> {
        let resume_point =
            UpdateResumePoint::new(self.current_version.to_string(), update.version.to_string())
                .with_running_bots(running_bots)
                .with_snowluma_daemon(snowluma_daemon_running);
        self.resume_store
            .save(&resume_point)
            .await
            .map_err(|e| UpdateError::ResumeError {
                reason: format!("save snapshot: {e}"),
            })?;

        match self.provider.download_and_install(&update).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // 失败时清理 snapshot, 避免下次启动误判"刚升级"
                let _ = self.resume_store.clear().await;
                self.record_failure(
                    UpdatePhase::Install,
                    format!("{e}"),
                    Some(update.version.to_string()),
                )
                .await;
                Err(e)
            }
        }
    }

    // ===== 4. resume_after_update =====

    /// 新版进程启动时调用; 返回 Some 表示刚升级,
    /// 上层(BotManager / SnowLumaDaemon)按 snapshot 还原状态后调 [Self::clear_resume]
    pub async fn resume_after_update(&self) -> Result<Option<UpdateResumePoint>, UpdateError> {
        self.resume_store.load().await
    }

    /// 还原完成后清理 snapshot
    pub async fn clear_resume(&self) -> Result<(), UpdateError> {
        self.resume_store.clear().await
    }

    // ===== 5. record_failure / detect_pending_failures =====

    /// 追加一条失败记录到 update-failures.jsonl; 写入失败只 warn 不传播
    pub async fn record_failure(
        &self,
        phase: UpdatePhase,
        error: impl Into<String>,
        target_version: Option<String>,
    ) {
        let mut failure = RecordedFailure::new(phase, error);
        if let Some(v) = target_version {
            failure = failure.with_target_version(v);
        }

        if let Err(e) = self.append_failure_line(&failure).await {
            tracing::warn!(
                target: "ncd_update",
                "failed to append update-failures.jsonl: {e}"
            );
        }
    }

    /// 启动时检查是否有未上报的失败记录
    pub async fn detect_pending_failures(&self) -> Result<Vec<RecordedFailure>, UpdateError> {
        match fs::read_to_string(&self.failures_path).await {
            Ok(content) => {
                let mut out = Vec::new();
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(f) = serde_json::from_str::<RecordedFailure>(line) {
                        out.push(f);
                    }
                }
                Ok(out)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(UpdateError::Io(e)),
        }
    }

    async fn append_failure_line(&self, failure: &RecordedFailure) -> Result<(), UpdateError> {
        if let Some(parent) = self.failures_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }
        let line = format!("{}\n", serde_json::to_string(failure)?);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.failures_path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockUpdateProvider;
    use chrono::Utc;
    use ncd_domain::SchemaVersion;
    use tempfile::tempdir;

    fn fake_update(version: &str, schema: SchemaVersion) -> AvailableUpdate {
        AvailableUpdate {
            v: 1,
            version: Version::parse(version).unwrap(),
            schema_version: schema,
            notes: "release notes".into(),
            pub_date: Utc::now(),
            download_url: "https://example.com/update.msi".into(),
            signature: String::new(),
            content_sha256: String::new(),
        }
    }

    fn make_orch(provider: Arc<MockUpdateProvider>, dir: &std::path::Path) -> UpdateOrchestrator {
        UpdateOrchestrator::new(
            provider,
            dir,
            Version::parse("0.1.0").unwrap(),
            SchemaVersion::V3,
        )
    }

    #[tokio::test]
    async fn check_returns_update_when_provider_yields_one() {
        let provider = Arc::new(MockUpdateProvider::new());
        provider.set_next_check(Ok(Some(fake_update("0.2.0", SchemaVersion::V3))));
        let dir = tempdir().unwrap();
        let orch = make_orch(provider.clone(), dir.path());
        let r = orch.check(UpdateChannel::Stable).await.unwrap();
        assert!(r.is_some());
        assert_eq!(r.unwrap().version, Version::parse("0.2.0").unwrap());
    }

    #[tokio::test]
    async fn check_filters_lower_version() {
        let provider = Arc::new(MockUpdateProvider::new());
        provider.set_next_check(Ok(Some(fake_update("0.0.5", SchemaVersion::V3))));
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let r = orch.check(UpdateChannel::Stable).await.unwrap();
        assert!(r.is_none()); // 0.0.5 < 0.1.0, 被过滤
    }

    #[tokio::test]
    async fn check_filters_same_version() {
        let provider = Arc::new(MockUpdateProvider::new());
        provider.set_next_check(Ok(Some(fake_update("0.1.0", SchemaVersion::V3))));
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let r = orch.check(UpdateChannel::Stable).await.unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn precheck_same_schema_is_ok() {
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let report = orch
            .precheck(&fake_update("0.2.0", SchemaVersion::V3))
            .await
            .unwrap();
        assert!(report.can_upgrade);
        assert!(report.warnings.is_empty());
    }

    #[tokio::test]
    async fn precheck_lower_schema_is_blocked() {
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let report = orch
            .precheck(&fake_update("0.2.0", SchemaVersion::V1))
            .await
            .unwrap();
        assert!(!report.can_upgrade);
        assert!(!report.blocking.is_empty());
    }

    #[tokio::test]
    async fn precheck_large_gap_is_blocked() {
        // current=V3, target=V6 gap=3
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let target = SchemaVersion::new(6);
        let report = orch.precheck(&fake_update("1.0.0", target)).await.unwrap();
        assert!(!report.can_upgrade);
        assert!(report.blocking.iter().any(|b| b.contains("gap")));
    }

    #[tokio::test]
    async fn precheck_small_gap_warns() {
        // current=V3, target=V4 gap=1
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        let target = SchemaVersion::new(4);
        let report = orch.precheck(&fake_update("1.0.0", target)).await.unwrap();
        assert!(report.can_upgrade);
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.estimated_migration_time_ms, 200);
    }

    #[tokio::test]
    async fn install_saves_resume_snapshot_before_calling_provider() {
        let provider = Arc::new(MockUpdateProvider::new());
        provider.set_next_install(Ok(()));
        let dir = tempdir().unwrap();
        let orch = make_orch(provider.clone(), dir.path());

        let update = fake_update("0.2.0", SchemaVersion::V3);
        orch.install_with_graceful_shutdown(
            update,
            vec!["100001".to_string(), "100002".to_string()],
            true,
        )
        .await
        .unwrap();

        // 检查 snapshot 已写盘
        let snap = orch.resume_after_update().await.unwrap();
        assert!(snap.is_some());
        let p = snap.unwrap();
        assert_eq!(p.from_version, "0.1.0");
        assert_eq!(p.to_version, "0.2.0");
        assert_eq!(p.running_bots.len(), 2);
        assert!(p.snowluma_daemon_running);
    }

    #[tokio::test]
    async fn install_failure_clears_snapshot_and_records_failure() {
        let provider = Arc::new(MockUpdateProvider::new());
        provider.set_next_install(Err(UpdateError::install_failed("simulated")));
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());

        let result = orch
            .install_with_graceful_shutdown(
                fake_update("0.2.0", SchemaVersion::V3),
                Vec::new(),
                false,
            )
            .await;
        assert!(result.is_err());
        // snapshot 应被清理
        assert!(orch.resume_after_update().await.unwrap().is_none());
        // 失败应记录
        let pending = orch.detect_pending_failures().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].phase, UpdatePhase::Install);
        assert!(pending[0].error.contains("simulated"));
        assert_eq!(pending[0].target_version.as_deref(), Some("0.2.0"));
    }

    #[tokio::test]
    async fn resume_returns_none_when_no_snapshot() {
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        assert!(orch.resume_after_update().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn clear_resume_idempotent() {
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        orch.clear_resume().await.unwrap();
        orch.clear_resume().await.unwrap();
    }

    #[tokio::test]
    async fn record_failure_appends_jsonl() {
        let provider = Arc::new(MockUpdateProvider::new());
        let dir = tempdir().unwrap();
        let orch = make_orch(provider, dir.path());
        orch.record_failure(UpdatePhase::Check, "404", None).await;
        orch.record_failure(UpdatePhase::Install, "msi 1603", Some("0.2.0".into()))
            .await;
        let pending = orch.detect_pending_failures().await.unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].phase, UpdatePhase::Check);
        assert_eq!(pending[1].phase, UpdatePhase::Install);
    }

    #[tokio::test]
    async fn detect_pending_failures_skips_invalid_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("update-failures.jsonl");
        fs::write(
            &path,
            "{not json}\n{\"timestamp\":\"2026-05-24T12:00:00Z\",\"phase\":\"check\",\"error\":\"x\"}\n",
        )
        .await
        .unwrap();
        let orch = UpdateOrchestrator::new(
            Arc::new(MockUpdateProvider::new()),
            dir.path(),
            Version::parse("0.1.0").unwrap(),
            SchemaVersion::V3,
        );
        let pending = orch.detect_pending_failures().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].phase, UpdatePhase::Check);
    }
}
