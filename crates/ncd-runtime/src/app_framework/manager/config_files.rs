//! 配置文件原文编辑，不改写框架字段或自动重新对接。

use super::*;
use ncd_appframework::config_doc::{read_document, write_document_text};
use ncd_domain::{AppConfigDocument, AppConfigText};

impl AppManager {
    pub async fn list_config_documents(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        let _guard = self.framework_config_gate.lock().await;
        let (instance, adapter, host) = self.config_file_context(id).await?;
        adapter
            .list_config_documents(host.as_ref(), &instance)
            .await
    }

    pub async fn read_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let _guard = self.framework_config_gate.lock().await;
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
    }

    pub async fn write_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
        text: &str,
        base_revision: Option<String>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let _guard = self.framework_config_gate.lock().await;
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
