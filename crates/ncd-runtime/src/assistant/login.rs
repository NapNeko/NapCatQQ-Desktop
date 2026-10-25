//! 只由桌面用户发起的 Agent 独立认证引导。

use std::sync::atomic::Ordering;

use ncd_domain::LocalShellKind;
use ncd_domain::assistant::AssistantLoginPlan;

use super::AssistantManager;

impl AssistantManager {
    pub async fn login_plan(&self, session_id: &str) -> Result<AssistantLoginPlan, String> {
        self.ensure_loaded().await?;
        if !cfg!(windows) {
            return Err("此登录引导需要 Windows PowerShell".into());
        }
        let session = self.session(session_id)?;
        if session.running.load(Ordering::Acquire) || session.stopping.load(Ordering::Acquire) {
            return Err("请先停止当前诊断，再登录 Agent".into());
        }
        let workspace = self.storage.workspace(session_id)?;
        tokio::fs::create_dir_all(&workspace)
            .await
            .map_err(|_| "创建助手登录目录失败")?;
        let provider = session.snapshot().info.provider;
        let settings = self.settings().await?;
        let command = super::providers::login_command(provider, &settings, &workspace)
            .await?
            .ok_or("请通过此 Agent 自己的登录流程完成认证")?;
        Ok(AssistantLoginPlan {
            command,
            shell: LocalShellKind::WindowsPowershell,
        })
    }
}
