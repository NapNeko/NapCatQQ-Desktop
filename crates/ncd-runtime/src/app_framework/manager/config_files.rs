//! 配置文件原文编辑，不改写框架字段或自动重新对接。

use super::*;
use ncd_appframework::config_doc::{read_document, write_document_text};
use ncd_domain::{AppConfigDocument, AppConfigText};

impl AppManager {
    pub async fn list_config_documents(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        read_with_deadline(id, "list", async {
            let _guard = self.framework_config_gate.operation(id.as_str()).await?;
            let (instance, adapter, host) = self.config_file_context(id).await?;
            adapter
                .list_config_documents(host.as_ref(), &instance)
                .await
        })
        .await
    }

    pub async fn read_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        read_with_deadline(id, "read", async {
            let _guard = self.framework_config_gate.operation(id.as_str()).await?;
            let (instance, adapter, host) = self.config_file_context(id).await?;
            let doc = resolve_document(adapter.as_ref(), host.as_ref(), &instance, doc_id).await?;
            let snapshot = read_document(
                host.as_ref(),
                &HostPath::from_posix(&instance.install_dir),
                &doc,
            )
            .await?;
            Ok(AppConfigText {
                doc_id: doc.id,
                text: snapshot.text.unwrap_or_default(),
                revision: snapshot.revision,
            })
        })
        .await
    }

    pub async fn write_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
        text: &str,
        base_revision: Option<String>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let _guard = self.framework_config_gate.operation(id.as_str()).await?;
        self.ensure_framework_write_allowed().await?;
        let (instance, adapter, host) = self.config_file_context(id).await?;
        adapter.validate_config_file_write(&instance, doc_id)?;
        let doc = resolve_document(adapter.as_ref(), host.as_ref(), &instance, doc_id).await?;
        let result = write_document_text(
            host.as_ref(),
            &HostPath::from_posix(&instance.install_dir),
            &doc,
            text,
            base_revision.as_deref(),
            ncd_appframework::adopt::write_project_sidecar(&instance),
        )
        .await;
        self.forget_webui_endpoint(id);
        result
    }

    async fn config_file_context(
        &self,
        id: &AppInstanceId,
    ) -> Result<(AppInstance, Arc<dyn AppFrameworkAdapter>, Arc<dyn Host>), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation("应用实例尚未安装".into()));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, adapter, host))
    }
}

async fn read_with_deadline<T>(
    id: &AppInstanceId,
    operation: &'static str,
    read: impl std::future::Future<Output = Result<T, AppFrameworkError>>,
) -> Result<T, AppFrameworkError> {
    let started = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();
    tracing::debug!(%request_id, instance = id.as_str(), operation, "开始读取配置");
    let result = match tokio::time::timeout(std::time::Duration::from_secs(8), read).await {
        Ok(result) => result,
        Err(_) => Err(AppFrameworkError::Host("读取配置超时，请重试".into())),
    };
    tracing::debug!(%request_id, instance = id.as_str(), operation, elapsed_ms = started.elapsed().as_millis(), success = result.is_ok(), "配置读取结束");
    result
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test(start_paused = true)]
    async fn stalled_read_releases_its_operation_when_the_page_budget_expires() {
        struct Finished(Arc<AtomicBool>);
        impl Drop for Finished {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let finished = Arc::new(AtomicBool::new(false));
        let gate = super::super::config_gate::ConfigGate::default();
        let result = read_with_deadline(&AppInstanceId::new("app"), "read", async {
            let _guard = gate.operation("app").await?;
            let _finished = Finished(Arc::clone(&finished));
            std::future::pending::<Result<(), AppFrameworkError>>().await
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("超时"));
        assert!(finished.load(Ordering::SeqCst));
        assert!(gate.operation("app").await.is_ok());
    }
}

async fn resolve_document(
    adapter: &dyn AppFrameworkAdapter,
    host: &dyn Host,
    instance: &AppInstance,
    doc_id: &str,
) -> Result<AppConfigDocument, AppFrameworkError> {
    adapter
        .list_config_documents(host, instance)
        .await?
        .into_iter()
        .find(|doc| doc.id == doc_id)
        .ok_or_else(|| AppFrameworkError::Validation(format!("未知的配置文件：{doc_id}")))
}
