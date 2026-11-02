//! Koishi 安装、WebUI 入口与支持运行中修改的 OneBot 对接。

mod bundle;
pub mod component;
pub mod console;
pub mod integration;
pub mod live;
pub mod manifest;
pub mod release;
pub mod yarn;
pub mod yml;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigFormat, AppFrameworkManifest, AppInstance, AppInstanceState,
    OneBotLinkPlan, TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};
use serde_json::Value;

pub use component::KoishiComponent;
pub use integration::KoishiIntegration;
pub use manifest::{KOISHI_FRAMEWORK_ID, koishi_manifest};
pub use yml::{KoishiInstanceConfig, KoishiPluginNode};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{DocumentSnapshot, read_document};
use console::KoishiConsole;
use live::{LiveOp, plan_live_ops};
use manifest::{
    ADAPTER_GROUP_IDENT, KOISHI_STDOUT_LOG, KOISHI_YML, LINK_IDENT, ONEBOT_ADAPTER_NAME,
    PACKAGE_JSON,
};
use yml::GROUP_NAME;

pub const DOC_KOISHI_YML: &str = "koishi";
pub const DOC_ENV: &str = "env";
pub const DOC_PACKAGE_JSON: &str = "package";

pub fn koishi_config_documents() -> Vec<AppConfigDocument> {
    vec![
        AppConfigDocument {
            id: DOC_KOISHI_YML.into(),
            label: KOISHI_YML.into(),
            rel_path: KOISHI_YML.into(),
            format: AppConfigFormat::Yaml,
            // Koishi 运行时会整份写回，原文只能停着改。
            hot_reload: false,
            reload_note: None,
        },
        AppConfigDocument {
            id: DOC_ENV.into(),
            label: ".env".into(),
            rel_path: ".env".into(),
            format: AppConfigFormat::DotEnv,
            hot_reload: false,
            reload_note: None,
        },
        AppConfigDocument {
            id: DOC_PACKAGE_JSON.into(),
            label: PACKAGE_JSON.into(),
            rel_path: PACKAGE_JSON.into(),
            format: AppConfigFormat::Json,
            hot_reload: false,
            reload_note: None,
        },
    ]
}

fn yml_doc() -> AppConfigDocument {
    koishi_config_documents().remove(0)
}

fn yml_path(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join(KOISHI_YML)
}

fn issues_error(issues: Vec<ncd_domain::AppConfigIssue>) -> AppFrameworkError {
    AppFrameworkError::ConfigInvalid(issues)
}

pub struct KoishiAdapter {
    integration: KoishiIntegration,
    console: KoishiConsole,
    /// 跑着的实例在桌面端这边的控制台回环口（远端经 SSH -L），由编排层告知
    live_ports: std::sync::Mutex<HashMap<String, u16>>,
}

impl Default for KoishiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl KoishiAdapter {
    pub fn new() -> Self {
        Self {
            integration: KoishiIntegration::new(),
            console: KoishiConsole::new(),
            live_ports: std::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn read_yml(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(KoishiInstanceConfig, DocumentSnapshot), AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snap = read_document(host, &root, &yml_doc()).await?;
        let Some(text) = snap.text.as_deref() else {
            return Err(AppFrameworkError::Integration(format!(
                "Koishi 实例缺少 koishi.yml（{}），请先完成安装",
                yml_path(instance).as_posix()
            )));
        };
        let cfg = KoishiInstanceConfig::parse(text).map_err(AppFrameworkError::Validation)?;
        Ok((cfg, snap))
    }

    async fn write_yml(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        cfg: &KoishiInstanceConfig,
    ) -> Result<(), AppFrameworkError> {
        let text = cfg.render().map_err(AppFrameworkError::Validation)?;
        let path = yml_path(instance);
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                host.write_file(&path, text.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }

    /// 跑着就给出控制台口；本机没登记过就用实例口（控制台和 server 同一个口）
    fn live_port(&self, instance: &AppInstance) -> Result<Option<u16>, AppFrameworkError> {
        if instance.state != AppInstanceState::Running {
            return Ok(None);
        }
        if let Some(p) = self
            .live_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(instance.id.as_str())
        {
            return Ok(Some(*p));
        }
        if matches!(instance.placement, ncd_domain::AppPlacement::LocalNative) {
            return Ok(Some(instance.port));
        }
        Err(AppFrameworkError::Runtime(
            "远端 Koishi 的控制台隧道还没开，稍后重试".into(),
        ))
    }

    async fn run_ops(
        &self,
        instance: &AppInstance,
        port: u16,
        ops: &[LiveOp],
    ) -> Result<(), AppFrameworkError> {
        let id = instance.id.as_str();
        for op in ops {
            match op {
                LiveOp::AppReload { .. } => {
                    self.console
                        .request_until_restart(id, port, op.kind(), op.args())
                        .await?;
                }
                _ => {
                    self.console.request(id, port, op.kind(), op.args()).await?;
                }
            }
        }
        Ok(())
    }

    /// koishi.yml 的唯一写入口：停着改文件，跑着交给控制台
    async fn mutate<F>(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        edit: F,
    ) -> Result<KoishiInstanceConfig, AppFrameworkError>
    where
        F: FnOnce(&mut KoishiInstanceConfig) -> Result<(), AppFrameworkError>,
    {
        let (before, _) = self.read_yml(host, instance).await?;
        let mut after = before.clone();
        edit(&mut after)?;
        let issues = after.validate();
        if !issues.is_empty() {
            return Err(issues_error(issues));
        }
        if after == before {
            return Ok(after);
        }
        match self.live_port(instance)? {
            Some(port) => {
                self.run_ops(instance, port, &plan_live_ops(&before, &after))
                    .await?
            }
            None => self.write_yml(host, instance, &after).await?,
        }
        Ok(after)
    }

    /// 对接条目放哪：模板自带、开着的 `group:adapter`，不然根上
    fn link_parent(cfg: &KoishiInstanceConfig) -> String {
        let usable = cfg
            .effective()
            .iter()
            .any(|n| n.name == GROUP_NAME && n.ident == ADAPTER_GROUP_IDENT);
        if usable {
            ADAPTER_GROUP_IDENT.to_string()
        } else {
            String::new()
        }
    }
}

#[async_trait]
impl AppFrameworkAdapter for KoishiAdapter {
    async fn read_webui_endpoint(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(u16, String), AppFrameworkError> {
        let (config, _) = self.read_yml(host, instance).await?;
        Ok((config.listen_port(), String::new()))
    }

    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(
            KoishiComponent::new(spec.install_dir.clone(), spec.port)
                .with_node_bin(spec.node_bin.clone())
                .with_npm_registry(spec.npm_registry.clone()),
        )
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        KoishiComponent::new(spec.install_dir.clone(), spec.port)
            .with_node_bin(spec.node_bin.clone())
            .with_npm_registry(spec.npm_registry.clone())
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        let mut path_prefix = vec![spec.install_dir.join("node_modules/.bin")];
        let node = yarn::resolve_node(host, &spec.install_dir, spec.node_bin.as_ref()).await;
        if let Some(dir) = node.as_ref().ok().and_then(|tc| tc.node_dir()) {
            path_prefix.push(dir);
        }
        let yarn_rel = yarn::resolve_yarn_rel(host, &spec.install_dir)
            .await
            .unwrap_or_else(|_| ".yarn/releases/yarn-4.12.0.cjs".into());
        let yarn = format!("node {yarn_rel}");
        let hint = if node.is_ok() {
            format!("node 已接好；yarn 是整包自带的那份，用 {yarn} 调")
        } else {
            "没找到实例用的 Node.js，yarn 可能用不了".to_string()
        };
        crate::terminal::AppTerminalProfile {
            path_prefix,
            env: Vec::new(),
            hint: Some(hint),
            snippets: vec![
                TerminalSnippet::new("装插件", format!("{yarn} add koishi-plugin-")),
                TerminalSnippet::new("看已装的插件", format!("{yarn} info --name-only")),
                TerminalSnippet::new("同步依赖", format!("{yarn} install")),
                TerminalSnippet::new("Node 版本", "node -v"),
            ],
        }
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let (cfg, _) = self.read_yml(host, instance).await?;
        Ok(cfg
            .find(ONEBOT_ADAPTER_NAME, LINK_IDENT)
            .and_then(|n| n.config.get("token"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (cfg, _) = self.read_yml(host, instance).await?;
        Ok(cfg
            .effective()
            .into_iter()
            .filter(|n| n.name == ONEBOT_ADAPTER_NAME)
            .filter(|n| n.config.get("protocol").and_then(Value::as_str) == Some("ws"))
            .filter_map(|n| n.config.get("endpoint").and_then(Value::as_str))
            .map(str::to_string)
            .collect())
    }

    fn wants_live_port(&self) -> bool {
        true
    }

    fn note_live_port(&self, instance_id: &str, port: u16) {
        self.live_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(instance_id.to_string(), port);
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let entry = integration::link_entry_config(plan.bot_id.as_str(), &plan.access_token);
        self.mutate(host, instance, |cfg| {
            if let Some(node) = cfg.find_mut(ONEBOT_ADAPTER_NAME, LINK_IDENT) {
                node.enabled = true;
                for (k, v) in entry {
                    node.config.insert(k, v);
                }
                return Ok(());
            }
            let parent = Self::link_parent(cfg);
            cfg.group_children_mut(&parent)
                .push(KoishiPluginNode::plugin(
                    ONEBOT_ADAPTER_NAME,
                    LINK_IDENT,
                    true,
                    entry,
                ));
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn unlink(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        self.mutate(host, instance, |cfg| {
            if let Some(node) = cfg.find_mut(ONEBOT_ADAPTER_NAME, LINK_IDENT) {
                node.enabled = false;
            }
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        if self.live_port(instance)?.is_some() {
            return self.unlink(host, instance).await;
        }
        restore_from_backup(host, &[yml_path(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(KOISHI_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        koishi_config_documents()
    }

    fn validate_config_file_write(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<(), AppFrameworkError> {
        if doc_id == DOC_KOISHI_YML && instance.state == AppInstanceState::Running {
            return Err(AppFrameworkError::Validation(
                "Koishi 运行时会覆盖 koishi.yml，请停止实例后保存，或在 WebUI 中修改。".into(),
            ));
        }
        Ok(())
    }
}

impl KoishiAdapter {}

#[cfg(all(test, windows))]
mod smoke;
#[cfg(test)]
mod tests;
