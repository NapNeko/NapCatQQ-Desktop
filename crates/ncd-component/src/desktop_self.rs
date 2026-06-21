//! DesktopSelfComponent:Desktop 自身的组件描述
//!
//! Desktop 自更新走 tauri-plugin-updater + 业务包装层 ncd-update,不复用
//! Component::install 流程(因为自更新涉及"自杀 + 重生",必须走平台原生 updater)
//!
//! 本 component 在 ncd-component 这层只提供:
//! - detect:读当前进程 exe 的版本号(从 cargo metadata 注入,通过
//!   env!("CARGO_PKG_VERSION"))
//! - verify:检查 exe 是否在期望路径
//! - launch_command:返回 self exe 路径(用于 ncd-update 在 SelfUpdate 后重启)
//! - install / update / uninstall:返回 Unsupported,引导调用方走 ncd-update
//!
//! 仅本地 + 自动 OS 检测:supported_targets 只声明 (任意 Os, Local) 三种,
//! Remote 永远拒绝

use async_trait::async_trait;

use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

use crate::context::ActionCtx;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// Desktop self component
#[derive(Debug, Clone)]
pub struct DesktopSelfComponent {
    /// Desktop 当前版本(由调用方注入,通常是 env!("CARGO_PKG_VERSION"))
    pub current_version: String,
    /// 当前 exe 的绝对路径(本地探测用)
    pub exe_path: HostPath,
}

impl DesktopSelfComponent {
    pub fn new(current_version: impl Into<String>, exe_path: HostPath) -> Self {
        Self {
            current_version: current_version.into(),
            exe_path,
        }
    }

    /// 用 std::env::current_exe() 自动获取 exe 路径(失败 fallback 到提示路径)
    pub fn from_env() -> Result<Self, ActionError> {
        let exe = std::env::current_exe().map_err(|e| ActionError::InvalidConfig {
            reason: format!("current_exe: {e}"),
        })?;
        let path_str = exe.to_string_lossy().into_owned();
        // Windows 路径转 HostPath
        let host_path = if cfg!(target_os = "windows") {
            HostPath::from_windows(&path_str)
        } else {
            HostPath::from_posix(path_str)
        };
        Ok(Self::new(env!("CARGO_PKG_VERSION"), host_path))
    }

    /// 组件元数据,给 list_components Tauri command 使用
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::DesktopSelf,
            display_name: "NapCatQQ Desktop".to_string(),
            description: "桌面端自身（自更新走 ncd-update）".to_string(),
            repo_url: Some("https://github.com/NapNeko/NapCatQQ-Desktop".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::MacOs, Locality::Local),
            ],
            category: crate::types::ComponentCategory::SelfApp,
        }
    }
}

#[async_trait]
impl Component for DesktopSelfComponent {
    fn id(&self) -> ComponentId {
        ComponentId::DesktopSelf
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // 自更新仅本地所有 OS 都支持(具体走哪个 updater 由 ncd-update 决定)
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::MacOs, Locality::Local),
        ]
    }

    async fn detect(&self, _host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        // Desktop 自身始终"已安装",直接返回当前版本
        Ok(Some(DetectedVersion {
            version: self.current_version.clone(),
            source: format!("{}", self.exe_path),
        }))
    }

    async fn install(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(
            "DesktopSelfComponent::install must go through ncd-update::UpdateOrchestrator",
        ))
    }

    async fn update(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(
            "DesktopSelfComponent::update must go through ncd-update::UpdateOrchestrator::install_with_graceful_shutdown",
        ))
    }

    async fn uninstall(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(
            "DesktopSelfComponent::uninstall is not supported (use OS uninstall flow)",
        ))
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let exists = host.exists(&self.exe_path).await?;
        Ok(VerifyReport::ok().with_check(
            "self exe exists",
            exists,
            Some(format!("{}", self.exe_path)),
        ))
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // 拼出"重启 desktop"的命令(供 ncd-update 在自更新完成后调用)
        Ok(args.apply_to(HostCommand::new(self.exe_path.as_posix())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> DesktopSelfComponent {
        DesktopSelfComponent::new(
            "0.1.0",
            HostPath::from_windows(r"C:\Program Files\NapCatQQ Desktop\NapCatQQ-Desktop.exe"),
        )
    }

    #[test]
    fn id_returns_desktop_self() {
        assert_eq!(comp().id(), ComponentId::DesktopSelf);
    }

    #[test]
    fn supported_targets_only_local() {
        let c = comp();
        let targets = c.supported_targets();
        assert!(targets.contains(&(Os::Windows, Locality::Local)));
        assert!(targets.contains(&(Os::Linux, Locality::Local)));
        assert!(targets.contains(&(Os::MacOs, Locality::Local)));
        // 任何 Remote 都不支持
        assert!(!targets.contains(&(Os::Windows, Locality::Remote)));
        assert!(!targets.contains(&(Os::Linux, Locality::Remote)));
    }

    #[test]
    fn from_env_returns_real_exe_path() {
        // current_exe 在测试 binary 上应能拿到一个绝对路径
        let c = DesktopSelfComponent::from_env().unwrap();
        assert!(!c.current_version.is_empty());
        assert!(c.exe_path.is_absolute() || !c.exe_path.as_posix().is_empty());
    }

    #[test]
    fn version_uses_cargo_pkg_version_in_from_env() {
        let c = DesktopSelfComponent::from_env().unwrap();
        assert_eq!(c.current_version, env!("CARGO_PKG_VERSION"));
    }
}
