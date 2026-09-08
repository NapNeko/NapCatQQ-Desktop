//! 框架适配器契约（Layer3）：把 Component / Integration / 应用端文件写入捏在一起。
//!
//! `ncd_traits::AppIntegration` 是纯计划；碰 Host 的部分（读 token / 写配置 / 回滚）
//! 放这里，因为 ncd-traits 不能依赖 ncd-host。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkManifest, AppInstance, OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, read_document, write_document_text,
};
use crate::karin::plugin::{
    KarinPluginInstalled, KarinPluginKind, KarinPluginMarketEntry, PluginLogSink,
};

/// 为某个应用实例构造 Component 所需的输入（编排层填）
#[derive(Debug, Clone)]
pub struct AppComponentSpec {
    /// 实例目录（HostPath）
    pub install_dir: HostPath,
    /// 实例监听端口（安装时写进应用端配置）
    pub port: u16,
    /// 桌面端管理的 Node 二进制（Node 系框架用）；None 则只看实例标记 / PATH
    pub node_bin: Option<HostPath>,
    /// 桌面端管理的 uv 二进制（Python 系框架用）；None 则只看实例标记 / PATH
    pub uv_bin: Option<HostPath>,
    /// npm registry 镜像；None 用默认源
    pub npm_registry: Option<String>,
    /// Karin：provision 时一并 `pnpm add @karinjs/plugin-puppeteer`。NoneBot2 忽略。
    pub install_renderer: bool,
}

#[async_trait]
pub trait AppFrameworkAdapter: Send + Sync {
    fn manifest(&self) -> &AppFrameworkManifest;

    /// 纯计划侧
    fn integration(&self) -> &dyn AppIntegration;

    /// 安装 / 探测 / 启动命令（R12：走既有 Component × Host × Action）
    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component>;

    /// 启动命令（可做 IO 解析工具链，比 `Component::launch_command` 的同步版准确）
    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError>;

    /// 应用端已配置的 OneBot 反向 WS token；没有或为空返回 None
    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError>;

    /// 把计划写进应用端：先备份 `<file>.ncd.bak`，任一步失败自动还原
    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError>;

    /// 解绑：应用端侧默认不动监听口（Bot 侧删连接即可）；有需要的框架覆写
    async fn unlink(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// `apply_link` 成功但 Bot 侧写失败时，把应用端文件从 `.ncd.bak` 还原
    async fn rollback_link(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// 实例运行日志文件（远端 tail 用）；None 表示只有进程 stdout
    fn log_file(&self, instance: &AppInstance) -> Option<HostPath>;

    /// 该实例可在「原始文件」Tab 编辑的配置文档；空表示没有可编辑配置
    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        Vec::new()
    }

    /// 类型化配置读取；没有类型化模型的框架保持默认（`ConfigUnsupported`）
    async fn read_config(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        Err(AppFrameworkError::ConfigUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 类型化配置写入（校验 → 备份写 → 回读）。返回写后信封，供编排层比对版本号
    async fn write_config(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        Err(AppFrameworkError::ConfigUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 读一份文档原文（缺文件返回空文本 + `"missing"` 版本号）
    async fn read_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let doc = self.find_document(instance, doc_id)?;
        let snap = read_document(host, &HostPath::from_posix(&instance.install_dir), &doc).await?;
        Ok(AppConfigText {
            doc_id: doc.id,
            text: snap.text.unwrap_or_default(),
            revision: snap.revision,
        })
    }

    /// 写一份文档原文（格式预检 + 版本号比对 + 备份写）
    async fn write_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
        text: &str,
        base_revision: Option<&str>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let doc = self.find_document(instance, doc_id)?;
        write_document_text(
            host,
            &HostPath::from_posix(&instance.install_dir),
            &doc,
            text,
            base_revision,
        )
        .await
    }

    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        self.config_documents(instance)
            .into_iter()
            .find(|d| d.id == doc_id)
            .ok_or_else(|| AppFrameworkError::Validation(format!("未知的配置文档: {doc_id}")))
    }

    async fn list_installed(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<KarinPluginInstalled>, AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn install_plugin(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _entry: &KarinPluginMarketEntry,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn update_plugin(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _entry: &KarinPluginMarketEntry,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn uninstall_plugin(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _name: &str,
        _kind: KarinPluginKind,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn list_plugin_config_docs(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }
}

/// 「备份 → 写 → 失败还原」骨架：`write` 返回 Err 时把 `files` 全部还原到备份内容。
/// 备份文件名 `<file>.ncd.bak`，成功后保留最近一份供人工比对。
pub async fn apply_with_backup<F, Fut>(
    host: &dyn Host,
    files: &[HostPath],
    write: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppFrameworkError>>,
{
    let mut originals: Vec<(HostPath, Option<Vec<u8>>)> = Vec::with_capacity(files.len());
    for file in files {
        let existed = host
            .exists(file)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        let content = if existed {
            let bytes = host
                .read_file(file)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            host.write_file(&backup_path(file), &bytes)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            Some(bytes.to_vec())
        } else {
            None
        };
        originals.push((file.clone(), content));
    }

    match write().await {
        Ok(()) => Ok(()),
        Err(err) => {
            for (file, original) in originals {
                let restore = match original {
                    Some(bytes) => host.write_file(&file, &bytes).await,
                    None => match host.exists(&file).await {
                        Ok(true) => host.remove_file(&file).await,
                        _ => Ok(()),
                    },
                };
                if let Err(e) = restore {
                    tracing::error!(
                        file = %file.as_posix(),
                        error = %e,
                        "restore app-side config after failed link"
                    );
                }
            }
            Err(err)
        }
    }
}

pub fn backup_path(file: &HostPath) -> HostPath {
    HostPath::from_posix(format!("{}.ncd.bak", file.as_posix()))
}

/// 把 `files` 各自从 `<file>.ncd.bak` 还原（没有备份的跳过）
pub async fn restore_from_backup(
    host: &dyn Host,
    files: &[HostPath],
) -> Result<(), AppFrameworkError> {
    for file in files {
        let backup = backup_path(file);
        let has_backup = host
            .exists(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if !has_backup {
            continue;
        }
        let bytes = host
            .read_file(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        host.write_file(file, &bytes)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    Ok(())
}
