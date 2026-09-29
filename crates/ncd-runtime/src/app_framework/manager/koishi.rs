//! Koishi 运行期接口：Bot 在线 / 性能、插件 schema 与已装列表、重启 worker。
//! 另有一个框架无关的小钩子 `prime_live_port`：跑着的实例改应用端配置要经控制台的框架，
//! 对接和商店操作之前先把桌面端这边的控制台口（远端是隧道口）告诉适配器。

use super::*;

fn koishi_api(
    adapter: &dyn AppFrameworkAdapter,
) -> Result<&dyn KoishiRuntimeApi, AppFrameworkError> {
    adapter.koishi_runtime().ok_or_else(|| {
        AppFrameworkError::ConfigUnsupported(adapter.manifest().id.as_str().to_string())
    })
}

impl AppManager {
    pub(super) async fn prime_live_port(
        &self,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        if !adapter.wants_live_port() || instance.state != AppInstanceState::Running {
            return Ok(());
        }
        let port = self.desktop_webui_loopback_port(instance).await?;
        adapter.note_live_port(instance.id.as_str(), port);
        Ok(())
    }

    /// 不报错：没在跑、隧道没通都折成 gate，概览照着出提示
    pub async fn koishi_status(
        &self,
        id: &AppInstanceId,
    ) -> Result<KoishiRuntimeStatus, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let api = koishi_api(self.registry.adapter(&instance.framework_id)?)?;
        if instance.state != AppInstanceState::Running {
            return Ok(KoishiRuntimeStatus::not_running());
        }
        let port = match self.desktop_webui_loopback_port(&instance).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(KoishiRuntimeStatus::gate(
                    KoishiRuntimeGate::Unreachable,
                    e.to_string(),
                ));
            }
        };
        Ok(api.status(&instance, port).await)
    }

    /// 插件表单（停着也能取：在实例目录里起 node 现读）；`names` 里的空串 = 全局设置
    pub async fn koishi_plugin_schemas(
        &self,
        id: &AppInstanceId,
        names: Vec<String>,
    ) -> Result<Vec<KoishiPluginSchema>, AppFrameworkError> {
        let (instance, adapter, host) = self.koishi_context(id).await?;
        koishi_api(adapter.as_ref())?
            .plugin_schemas(host.as_ref(), &instance, &names)
            .await
    }

    pub async fn koishi_packages(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<KoishiPackageInfo>, AppFrameworkError> {
        let (instance, adapter, host) = self.koishi_context(id).await?;
        koishi_api(adapter.as_ref())?
            .installed_packages(host.as_ref(), &instance)
            .await
    }

    /// 上游自己的重启：worker 退出码 51，daemon 重新拉起，桌面端记着的进程不变。
    /// 输出却换了一轮，日志面板跟着重新开始
    pub async fn koishi_restart(&self, id: &AppInstanceId) -> Result<(), AppFrameworkError> {
        let (instance, adapter, host) = self.koishi_context(id).await?;
        if instance.state != AppInstanceState::Running {
            return Err(AppFrameworkError::NotRunning("Koishi 没在运行".into()));
        }
        let port = self.desktop_webui_loopback_port(&instance).await?;
        koishi_api(adapter.as_ref())?
            .restart(host.as_ref(), &instance, port)
            .await?;
        let log_file = launch_log_file(adapter.as_ref(), &instance);
        if let Err(e) = self.runtime.reset_log(host, &instance, log_file).await {
            tracing::warn!(instance = id.as_str(), error = %e, "reset app log after koishi restart");
        }
        Ok(())
    }

    async fn koishi_context(
        &self,
        id: &AppInstanceId,
    ) -> Result<(AppInstance, Arc<dyn AppFrameworkAdapter>, Arc<dyn Host>), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, adapter, host))
    }
}
