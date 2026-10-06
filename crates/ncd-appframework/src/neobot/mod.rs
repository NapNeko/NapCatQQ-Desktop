//! NeoBot 适配器：manifest + Component + Integration + 类型化配置 + 导入探测。

pub mod component;
pub mod config;
pub mod control;
pub mod integration;
pub mod manifest;
pub mod probe;
pub mod versions;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppProjectProbe, OneBotLinkPlan,
    TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath, Locality};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::NeoBotComponent;
pub use config::{
    DOC_ADAPTER, DOC_DASHBOARD, NeoBotAdapterConfig, NeoBotDashboardConfig, NeoBotInstanceConfig,
    neobot_config_documents,
};
pub use control::{MIN_VERSION_WITH_SHUTDOWN_ENDPOINT, ShutdownRequest};
pub use integration::NeoBotIntegration;
pub use manifest::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID, neobot_manifest,
};
pub use probe::probe_neobot;
pub use versions::{PackageVersions, fetch_versions, parse_versions};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, AppPanelOutcomeKind, AppPanelResult, restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
};
use crate::terminal;

fn envelope(config: NeoBotInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::NeoBot(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

pub struct NeoBotAdapter {
    integration: NeoBotIntegration,
    /// 面板会话（token + CSRF）。适配器在注册表里活一个进程周期，所以这里缓存得住：
    /// 登录要过 PBKDF2-HMAC-SHA256（24 万次迭代），每个请求都重登一遍太浪费。
    sessions: std::sync::Arc<control::PanelSessions>,
}

impl Default for NeoBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NeoBotAdapter {
    pub fn new() -> Self {
        Self {
            integration: NeoBotIntegration::new(),
            sessions: std::sync::Arc::new(control::PanelSessions::new()),
        }
    }

    fn component_for(spec: &AppComponentSpec) -> NeoBotComponent {
        let dashboard_port = dashboard_port_for(spec);
        NeoBotComponent::new(spec.install_dir.clone(), spec.port, dashboard_port)
            .with_uv_bin(spec.uv_bin.clone())
            .with_pypi_index(spec.pypi_index.clone())
            .with_adopt_existing(spec.adopt_existing)
            .with_install_version(spec.install_version.clone())
    }

    fn install_dir(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir)
    }
}

/// 实例版本是否支持优雅关闭（`/api/admin/shutdown` 自 NeoBot 1.2.3 起提供）。
///
/// 版本读不出来（未知 / 装了但读不到）时**按不支持处理**：宁可退回原来的「请求没送达、
/// 直接收树」，也不要对着一个不存在的端点干等 15 分钟。
pub fn version_supports_graceful_stop(installed_version: Option<&str>) -> bool {
    let Some(raw) = installed_version.map(str::trim).filter(|v| !v.is_empty()) else {
        return false;
    };
    match crate::neobot::versions::parse_pep440(raw) {
        Some(v) => {
            v >= crate::neobot::versions::parse_pep440(control::MIN_VERSION_WITH_SHUTDOWN_ENDPOINT)
                .expect("常量可解析")
        }
        None => false,
    }
}

/// 面板口：目前没有从 spec 传进来的通道（`AppComponentSpec` 里没有这个字段），
/// 一律用出厂值；桌面端安装时已把真实面板口写进面板配置，之后读类型化配置就能拿到。
fn dashboard_port_for(_spec: &AppComponentSpec) -> u16 {
    manifest::NEOBOT_DEFAULT_DASHBOARD_PORT
}

#[async_trait]
impl AppFrameworkAdapter for NeoBotAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    /// NeoBot 发在 PyPI，可安装版本查 PyPI 的 JSON API
    async fn available_versions(
        &self,
    ) -> Result<Option<versions::PackageVersions>, AppFrameworkError> {
        versions::fetch_versions(manifest::PYPI_NEOBOT, None)
            .await
            .map(Some)
            .map_err(|e| AppFrameworkError::Integration(format!("查 PyPI 版本失败：{e}")))
    }

    /// NeoBot 面板转发：面板口从实例配置读（与 OneBot 口是两个口），
    /// 密码是用户在桌面端填过一次的那个（密钥库里的「面板密码」）。
    async fn panel_request(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        password: Option<&str>,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Option<AppPanelResult>, AppFrameworkError> {
        // 转发只连本机回环。远端实例的面板口读出来以后连的也是 127.0.0.1，
        // 打到的会是本机同口的另一个服务，还会把这台实例的面板密码交给它
        if host.locality() != Locality::Local {
            return Ok(Some(AppPanelResult::err(
                AppPanelOutcomeKind::Failed,
                "远端实例的面板还不能经桌面端读取，请用「打开控制台」在浏览器里使用面板",
            )));
        }
        let root = Self::install_dir(instance);
        let port =
            control::dashboard_port(host, &root, manifest::NEOBOT_DEFAULT_DASHBOARD_PORT).await;
        let outcome = control::panel_call(
            &self.sessions,
            instance.id.as_str(),
            port,
            password,
            method,
            path,
            body,
        )
        .await;
        Ok(Some(match outcome {
            control::PanelOutcome::Ok(v) => AppPanelResult::ok(v),
            control::PanelOutcome::Unauthorized => AppPanelResult::err(
                AppPanelOutcomeKind::Unauthorized,
                "面板凭据不可用：请先在本页填写面板密码",
            ),
            control::PanelOutcome::Unreachable(e) => {
                AppPanelResult::err(AppPanelOutcomeKind::Unreachable, format!("面板打不通：{e}"))
            }
            control::PanelOutcome::NotFound => AppPanelResult::err(
                AppPanelOutcomeKind::NotFound,
                "面板没有这个接口：当前 NeoBot 版本还不提供它，需要更新的版本",
            ),
            control::PanelOutcome::Failed(e) => AppPanelResult::err(AppPanelOutcomeKind::Failed, e),
        }))
    }

    fn forget_panel_session(&self, instance_id: &str) {
        self.sessions.clear(instance_id);
    }

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_neobot(host, path).await
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        let comp = Self::component_for(spec);
        let cmd = comp
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
        // 搬不成不拦启动：最坏是实例还按出厂口听着，用户改绑一次就好
        match config::carry_legacy_link(host, &spec.install_dir, spec.port).await {
            Ok(true) => tracing::info!(
                instance = spec.instance_id.as_str(),
                "moved neobot link keys from data/config.toml to app/data/config.toml"
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!(
                instance = spec.instance_id.as_str(),
                error = %e,
                "failed to move neobot link keys from legacy data/config.toml"
            ),
        }
        Ok(cmd)
    }

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> terminal::AppTerminalProfile {
        // 插件与依赖平时由面板自己装；终端里照同样的办法来
        let mut profile = terminal::uv_venv_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装了哪些包", "uv pip list"),
                TerminalSnippet::new("换一个版本", "uv pip install -U neobot-app"),
                TerminalSnippet::new("直接跑一次", "neobot"),
            ],
        )
        .await;
        // 终端里手动跑 neobot 也要读写同一份数据，和受管启动一致
        let comp = Self::component_for(spec);
        profile.env.extend(
            comp.path_env(host.os())
                .into_iter()
                .map(|(k, v)| (k.to_string(), v)),
        );
        profile
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        match config::read_neobot_config(host, &Self::install_dir(instance)).await {
            Ok((cfg, _)) => {
                let token = cfg.adapter.reverse_ws_access_token.trim();
                Ok((!token.is_empty()).then(|| token.to_string()))
            }
            // 没装好 / 文件坏了都不算「已有 token」
            Err(_) => Ok(None),
        }
    }

    /// 把对接写进 `app/data/config.toml` 的 `[adapter]`。
    ///
    /// 反向 WS：应用端是监听方，**口以实例口为准**（桌面端分配的实例口就是受管口，
    /// 也是 `listen_port()` 报给编排层的值），不拿 plan 里 URL 的端口去改写——
    /// 那个 url 是给协议 Bot 连的地址，跨机时还可能是隧道口。
    /// token 两边必须一致，所以写 plan 给的。
    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let root = Self::install_dir(instance);
        let (mut cfg, _) = config::read_neobot_config(host, &root).await.map_err(|e| {
            AppFrameworkError::Integration(format!("读 NeoBot 配置失败，无法对接：{e}"))
        })?;
        cfg.adapter.reverse_ws_access_token = plan.access_token.clone();
        if instance.port > 0 {
            cfg.adapter.reverse_ws_port = instance.port;
        }
        let issues = cfg.validate();
        if !issues.is_empty() {
            return Err(AppFrameworkError::ConfigInvalid(issues));
        }
        // write_link_config 内部走 apply_with_backup_ex：备份 -> 差量写 -> 失败还原；
        // 面板文件只在已存在时写，不给没装面板插件的领养项目新建（rollback 只还原本体配置）
        config::write_link_config(host, &root, &cfg, write_project_sidecar(instance))
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
        Ok(())
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(
            host,
            &[Self::install_dir(instance).join(manifest::NEOBOT_CONFIG_TOML)],
        )
        .await
    }

    /// 请 NeoBot 自己开始收尾。
    ///
    /// 返回 `Ok(true)` = 请求已送达，编排层据此把等待窗口拉满
    /// （`graceful_stop_timeout`）；`Ok(false)` = 没法请它退，直接收树。
    ///
    /// **版本闸门**：`/api/admin/shutdown` 是 NeoBot 1.2.3 才有的。更老的实例
    /// 调用只会拿到 404，与其白等不如直接返回 false 让编排层收树——
    /// 这也是"只应该写需要 xx 版本以上"的落点。
    async fn request_graceful_stop(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        panel_password: Option<&str>,
    ) -> Result<bool, AppFrameworkError> {
        // 优雅关闭打的是面板回环地址，只对同机实例有意义：远端实例不是打不通，
        // 就是误打到本机同口的另一个实例。远端支持得走编排层隧道，暂不提供；
        // 这里返回 false 让编排层直接收树
        if host.locality() != Locality::Local {
            tracing::info!(
                instance = instance.id.as_str(),
                "neobot graceful stop is loopback-only; skipping for remote instance"
            );
            return Ok(false);
        }
        if !version_supports_graceful_stop(instance.installed_version.as_deref()) {
            tracing::info!(
                instance = instance.id.as_str(),
                version = instance.installed_version.as_deref().unwrap_or("未知"),
                "neobot older than {}: no graceful stop endpoint, will kill the tree",
                control::MIN_VERSION_WITH_SHUTDOWN_ENDPOINT
            );
            return Ok(false);
        }
        // 面板口与实例口不同：必须读配置里的真实面板口，否则请求打到 OneBot 口上
        let root = Self::install_dir(instance);
        let port =
            control::dashboard_port(host, &root, manifest::NEOBOT_DEFAULT_DASHBOARD_PORT).await;
        // 面板密码由编排层从密钥库取来传进来（见 AppManager::stop_gracefully）：
        // 面板设过密码的实例不带它一律 401/403，优雅关闭就走不通了
        match control::request_graceful_shutdown(port, panel_password).await {
            control::ShutdownRequest::Accepted => Ok(true),
            control::ShutdownRequest::EndpointMissing => {
                tracing::info!(
                    instance = instance.id.as_str(),
                    "neobot has no shutdown endpoint yet, will kill the tree"
                );
                Ok(false)
            }
            control::ShutdownRequest::Unauthorized => {
                tracing::warn!(
                    instance = instance.id.as_str(),
                    "neobot panel rejected the shutdown request (password unknown), will kill the tree"
                );
                Ok(false)
            }
            control::ShutdownRequest::Unreachable(e) => {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "neobot panel unreachable");
                Ok(false)
            }
            control::ShutdownRequest::Failed(e) => {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "neobot shutdown request failed");
                Ok(false)
            }
        }
    }

    /// 优雅关闭的等待窗口。NeoBot 关闭要跑记忆总结（上游 `[standby]`
    /// `shutdown_timeout_seconds` 默认 300 秒），15 秒的默认值会在收尾中途收树，
    /// 所以这里给 15 分钟——比上游窗口宽裕，留出进程退出与文件落盘的时间。
    fn graceful_stop_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(15 * 60)
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(Self::install_dir(instance).join(manifest::NEOBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        neobot_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (cfg, snaps) = config::read_neobot_config(host, &Self::install_dir(instance)).await?;
        Ok(envelope(cfg, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::NeoBot(next) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 NeoBot 配置".to_string(),
            ));
        };
        let root = Self::install_dir(instance);
        let (cfg, snaps) =
            config::write_neobot_config(host, &root, next, write_project_sidecar(instance)).await?;
        Ok(envelope(cfg, &snaps))
    }

    fn webui_fallback_port(&self, _instance: &AppInstance) -> u16 {
        manifest::NEOBOT_DEFAULT_DASHBOARD_PORT
    }

    fn validate_webui_password(&self, password: &str) -> Result<(), String> {
        if password.chars().count() < 8 {
            return Err("面板密码至少 8 位".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本闸门：只有 1.2.3 及以上才走优雅关闭，其余返回 false 让编排层直接收树。
    /// 这是「只应该写需要 xx 版本以上」的落点。
    #[test]
    fn graceful_stop_requires_min_version() {
        let gate = version_supports_graceful_stop;
        assert!(gate(Some("1.2.3")));
        assert!(gate(Some("1.2.4")));
        assert!(gate(Some("1.3.0")));
        assert!(gate(Some("2.0.0")));
        assert!(gate(Some(" 1.2.3 ")), "两侧空白要吃掉");
        // 同一 release 段的预发布仍低于正式版：1.2.3a1 < 1.2.3
        assert!(!gate(Some("1.2.3a1")));
        assert!(!gate(Some("1.2.2")));
        assert!(!gate(Some("1.2.0")));
        assert!(!gate(Some("1.0.0")));
        // 读不出实例版本时按不支持处理，避免对着不存在的端点干等整个窗口
        assert!(!gate(None));
        assert!(!gate(Some("")));
        assert!(!gate(Some("   ")));
        assert!(
            !gate(Some("installed")),
            "旧适配器留下的字面量不能被当成版本号"
        );
    }
}
