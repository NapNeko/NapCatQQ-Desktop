//! Windows 原生远端监督器的安装、升级与协议就绪探测。
use crate::{
    ActionCtx, ActionError, Component, ComponentCategory, ComponentId, ComponentInfo,
    DetectedVersion, LaunchArgs, SupportedTarget, UninstallSupport, VerifyReport,
};
use async_trait::async_trait;
use ncd_domain::windows_remote::{
    WINDOWS_AGENT_PROTOCOL, WindowsAgentAction, WindowsAgentRequest, WindowsAgentResult,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use std::path::PathBuf;

pub struct WindowsAgentComponent {
    version: String,
    binary: Option<PathBuf>,
}
pub const WINDOWS_AGENT_BUNDLED_VERSION: &str = "0.1.0";

impl WindowsAgentComponent {
    pub fn bundled() -> Self {
        Self::new(WINDOWS_AGENT_BUNDLED_VERSION)
    }
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            binary: discover_local_windows_agent_binary(),
        }
    }
    pub fn info() -> ComponentInfo {
        ComponentInfo {
            id: ComponentId::WindowsAgent,
            display_name: "Windows 远程代理".into(),
            description: String::new(),
            repo_url: None,
            supported_targets: vec![SupportedTarget::new(Os::Windows, Locality::Remote)],
            category: ComponentCategory::RuntimeDep,
            uninstall: UninstallSupport::Supported,
        }
    }
}

pub fn discover_local_windows_agent_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    let mut candidates = Vec::new();
    #[cfg(debug_assertions)]
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri/resources/remote-agent/ncd-agent.exe"));
        candidates.push(cwd.join("resources/remote-agent/ncd-agent.exe"));
    }
    candidates.extend([
        parent.join("ncd-agent.exe"),
        parent.join("resources/ncd-agent.exe"),
        parent.join("binaries/ncd-agent.exe"),
    ]);
    #[cfg(debug_assertions)]
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("target/debug/ncd-agent.exe"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[async_trait]
impl Component for WindowsAgentComponent {
    fn id(&self) -> ComponentId {
        ComponentId::WindowsAgent
    }
    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[(Os::Windows, Locality::Remote)]
    }
    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        self.check_target(host)?;
        let info = host.remote_windows_info().await?;
        let root = HostPath::from_windows(&info.managed_root);
        if !host
            .exists(&root.join("agent/current/ncd-agent.exe"))
            .await?
        {
            return Ok(None);
        }
        let response = host
            .windows_agent_request(WindowsAgentRequest {
                v: WINDOWS_AGENT_PROTOCOL,
                request_id: format!("component-{}", std::process::id()),
                action: WindowsAgentAction::Hello,
            })
            .await?;
        match response.result {
            WindowsAgentResult::Hello(status)
                if status.v == WINDOWS_AGENT_PROTOCOL && status.service_running =>
            {
                Ok(Some(DetectedVersion {
                    version: status.version,
                    source: root.render_for(Os::Windows),
                }))
            }
            WindowsAgentResult::Error(error) => Err(ActionError::other(error.message)),
            _ => Err(ActionError::other("远端 Windows 代理未就绪或协议不兼容")),
        }
    }
    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        crate::windows_install::require_x64(host, "Windows 远程代理")?;
        let info = host.remote_windows_info().await?;
        if !info.elevated {
            return Err(ActionError::other(
                "安装 Windows 远程代理需要目标机管理员 SSH 会话",
            ));
        }
        let source = self
            .binary
            .as_ref()
            .filter(|p| p.is_file())
            .ok_or_else(|| {
                ActionError::other("Desktop 安装中缺少 ncd-agent.exe，请修复或更新 Desktop")
            })?;
        let root = HostPath::from_windows(&info.managed_root);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let stage = root
            .join("agent/staging")
            .join(format!("{}-{nonce}", std::process::id()));
        host.create_dir_all(&stage).await?;
        let binary = stage.join("ncd-agent.exe");
        host.upload(source, &binary).await?;
        if ctx.is_cancelled() {
            let _ = host.remove_dir_all(&stage).await;
            return Err(ActionError::Cancelled);
        }
        ctx.info(format!("安装 Windows 远程代理 {}", self.version))
            .await;
        let out = host
            .run_to_string(
                HostCommand::new(binary.render_for(Os::Windows))
                    .args(["install", "--root"])
                    .arg(root.render_for(Os::Windows))
                    .arg("--owner-sid")
                    .arg(&info.user_sid)
                    .elevated(),
            )
            .await?;
        if !out.success() {
            return Err(ActionError::other(format!(
                "安装 Windows 远程代理失败：{}",
                out.stderr.trim()
            )));
        }
        let register = host
            .run_to_string(
                HostCommand::new(
                    root.join("agent/current/ncd-agent.exe")
                        .render_for(Os::Windows),
                )
                .args(["register-session", "--root"])
                .arg(root.render_for(Os::Windows)),
            )
            .await?;
        if !register.success() {
            return Err(ActionError::other(format!(
                "注册 Windows 登录会话失败：{}",
                register.stderr.trim()
            )));
        }
        let _ = host.remove_dir_all(&stage).await;
        if self.detect(host).await?.is_none() {
            return Err(ActionError::other("Windows 远程代理安装后未就绪"));
        }
        ctx.emit(crate::ProgressKind::Finished { ok: true }).await;
        Ok(())
    }
    async fn uninstall(&self, host: &dyn Host, _ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let info = host.remote_windows_info().await?;
        let root = HostPath::from_windows(&info.managed_root);
        let binary = root.join("agent/current/ncd-agent.exe");
        let out = host
            .run_to_string(
                HostCommand::new(binary.render_for(Os::Windows))
                    .args(["uninstall", "--root"])
                    .arg(root.render_for(Os::Windows))
                    .elevated(),
            )
            .await?;
        if !out.success() {
            return Err(ActionError::other(format!(
                "卸载 Windows 远程代理失败：{}",
                out.stderr.trim()
            )));
        }
        Ok(())
    }
    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let found = self.detect(host).await?;
        Ok(VerifyReport::ok().with_check(
            "Windows 代理服务",
            found.is_some(),
            found.map(|f| f.version),
        ))
    }
    fn launch_command(
        &self,
        _host: &dyn Host,
        _args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        Err(ActionError::other("Windows 远程代理由系统服务管理"))
    }
}
