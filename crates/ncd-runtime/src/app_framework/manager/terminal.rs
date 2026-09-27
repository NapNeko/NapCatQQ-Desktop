//! 终端开在实例目录：实例在哪台主机、目录在哪、框架要接上什么环境

use std::sync::Arc;

use ncd_appframework::AppTerminalProfile;
use ncd_domain::{AppInstance, AppInstanceId};
use ncd_host::Host;
use ncd_traits::AppFrameworkError;

use super::AppManager;

/// 开实例终端要的全部信息
pub struct AppTerminalContext {
    pub instance: AppInstance,
    pub host: Arc<dyn Host>,
    /// 框架显示名（麦麦 / Karin …）
    pub framework_name: String,
    pub profile: AppTerminalProfile,
}

impl AppManager {
    /// 工具链的找法和启动实例时一样，终端里敲的 node / uv 和实例跑起来用的是同一个
    pub async fn terminal_context(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppTerminalContext, AppFrameworkError> {
        let instance = self.get_instance(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        let spec = self.component_spec(host.as_ref(), &instance);
        let profile = adapter.terminal_profile(host.as_ref(), &spec).await;
        Ok(AppTerminalContext {
            framework_name: adapter.manifest().display_name.clone(),
            instance,
            host,
            profile,
        })
    }
}
