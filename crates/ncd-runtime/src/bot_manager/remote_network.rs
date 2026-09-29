// 已在桌面端的远端 Bot 回读远端框架的网络配置。
// 桌面端每次启动远端 Bot 都会按 bot.json 重写 onebot 文件，用户在远端 WebUI 里改的
// 连接会被覆盖；这里给用户一个启动前先拉回来的机会。只读不写，合并和保存由前端确认后走
// 正常的 upsert。

use super::*;
use ncd_domain::{DeploymentType, ImportedNetworkConfig};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 读远端这个 Bot 当前的 onebot 网络配置。Ok(None) 表示远端没有这份文件。
    pub async fn fetch_remote_network(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<ImportedNetworkConfig>, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        let RuntimeTarget::Server(server_id) = &config.bot.runtime_target else {
            return Err(BotManagerError::RemoteRead(
                "本机 Bot 的配置文件就在本机，不需要从远端读取".into(),
            ));
        };
        let resolver = self
            .host_resolver
            .as_ref()
            .ok_or_else(|| BotManagerError::RemoteRead("HostResolver 未初始化".into()))?;
        let host = resolver
            .resolve(&config.bot.runtime_target)
            .await
            .map_err(|e| BotManagerError::RemoteRead(format!("连接远端主机失败: {e}")))?;
        // 和启动走同一份库存，避免读的路径和启动时写的路径不是一处
        let selected = self
            .runtime_router()
            .selected_for_server(server_id, host.as_ref())
            .await
            .ok_or_else(|| {
                BotManagerError::RemoteRead(
                    "尚未发现该主机的安装库存，请先在服务器页重新发现".into(),
                )
            })?;

        let docker_name = match config.bot.deployment_type {
            DeploymentType::Docker => Some(
                ncd_deploy::resolve_bot_container_name(host.as_ref(), bot_id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| ncd_deploy::DockerDeployment::container_name(&config)),
            ),
            DeploymentType::Native => None,
        };

        crate::remote::import_network::fetch_imported_network(
            host.as_ref(),
            &selected,
            config.bot.backend_type,
            config.bot.deployment_type,
            bot_id.as_str(),
            docker_name.as_deref(),
        )
        .await
        .map_err(BotManagerError::RemoteRead)
    }
}
