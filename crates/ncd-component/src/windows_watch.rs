//! Windows 值守由机器 Agent 承载，组件只控制该用户的启用标记。
use crate::{
    ActionCtx, ActionError, Component, ComponentId, DetectedVersion, LaunchArgs, VerifyReport,
};
use async_trait::async_trait;
use ncd_domain::windows_remote::{WindowsAgentAction, WindowsAgentRequest, WindowsAgentResult};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

pub struct WindowsNcdWatchComponent;

impl WindowsNcdWatchComponent {
    fn root(host: &dyn Host) -> Result<HostPath, ActionError> {
        host.target_managed_root()
            .map(|root| root.join("ncd-watch"))
            .ok_or_else(|| ActionError::other("远端 Windows 用户数据根不可用"))
    }
    async fn agent(host: &dyn Host) -> Result<String, ActionError> {
        let response = host
            .windows_agent_request(WindowsAgentRequest {
                v: 1,
                request_id: format!("watch-{}", std::process::id()),
                action: WindowsAgentAction::Hello,
            })
            .await?;
        match response.result {
            WindowsAgentResult::Hello(status)
                if status.service_running
                    && status
                        .capabilities
                        .iter()
                        .any(|feature| feature == "watch_v1") =>
            {
                Ok(status.version)
            }
            WindowsAgentResult::Error(error) => Err(ActionError::other(error.message)),
            _ => Err(ActionError::other("请先安装支持值守的 Windows 远程代理")),
        }
    }
}

#[async_trait]
impl Component for WindowsNcdWatchComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NcdWatch
    }
    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[(Os::Windows, Locality::Remote)]
    }
    fn requirements(&self, _: Os, _: Locality) -> Vec<crate::Requirement> {
        vec![crate::Requirement::component(ComponentId::WindowsAgent)]
    }
    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        self.check_target(host)?;
        let root = Self::root(host)?;
        if host.exists(&root.join("config/disabled")).await? {
            return Ok(None);
        }
        match Self::agent(host).await {
            Ok(version) => Ok(Some(DetectedVersion {
                version,
                source: "Windows 远程代理内置值守".into(),
            })),
            Err(_) => Ok(None),
        }
    }
    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        Self::agent(host).await?;
        let config = Self::root(host)?.join("config");
        host.create_dir_all(&config).await?;
        let disabled = config.join("disabled");
        if host.exists(&disabled).await? {
            host.remove_file(&disabled).await?;
        }
        ctx.info("已启用 Windows Agent 内置值守").await;
        Ok(())
    }
    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.install(host, ctx).await
    }
    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        let config = Self::root(host)?.join("config");
        host.create_dir_all(&config).await?;
        host.write_file(&config.join("disabled"), b"disabled\n")
            .await?;
        ctx.info("已停用该用户的远端值守，保留通知记录").await;
        Ok(())
    }
    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let active = self.detect(host).await?.is_some();
        Ok(VerifyReport::ok().with_check("Agent watch enabled", active, None))
    }
    fn launch_command(&self, _: &dyn Host, _: &LaunchArgs) -> Result<HostCommand, ActionError> {
        Err(ActionError::other("Windows 值守由远程代理服务管理"))
    }
}
