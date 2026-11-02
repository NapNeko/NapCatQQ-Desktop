//! 运行中 NapCat 与 SnowLuma 的配置热更新。

use super::BotManager;
use crate::napcat::endpoint_table::NapCatEndpoint;
use crate::snowluma::{ReqwestSnowLumaWebUiClient, SnowLumaWebUiClient};
use ncd_domain::DeploymentType;
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_traits::{BotConfigRepo, ConfigStore};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 把一份 OneBot 配置 payload 通过 SnowLuma WebUI 热推送给运行中的 bot。
    /// 本机与远端共用同一条 reason 语义:
    /// - config_hot_reloaded:推送成功(上游返回 reloaded=true,在线热重载)
    /// - config_saved_pending_reload:离线 / 网络 / 鉴权 / 业务错误,等下次重启生效
    /// 本机走 snowluma_daemon 的共享 client;远端按 server_id 查 remote_snowluma_daemons,
    /// 从隧道端点构造临时 ReqwestSnowLumaWebUiClient 再调 update_onebot_config。
    pub(super) async fn push_snowluma_hot_reload(
        &self,
        config: &BotConfig,
        uin: &str,
        payload: &serde_json::Value,
    ) -> &'static str {
        if config.bot.deployment_type == DeploymentType::Docker {
            let bot_id = BotId::new(config.bot.qq_id.to_string());
            let Some((client, _, _)) = self
                .remote_runtime_sessions()
                .snowluma_docker_client(&bot_id)
                .await
            else {
                return "config_saved_pending_reload";
            };
            return match client.update_onebot_config(uin, payload).await {
                Ok(true) => "config_hot_reloaded",
                _ => "config_saved_pending_reload",
            };
        }
        // 本机:daemon Ready 时直接用共享 client,client 内部已 login 持 token
        if config.bot.runtime_target.is_local() {
            let Some(daemon) = &self.snowluma_daemon else {
                return "config_saved_pending_reload";
            };
            let client = match daemon.current_client().await {
                Ok(c) => c,
                Err(err) => {
                    tracing::warn!(error = ?err, "snowluma hot reload: daemon not ready");
                    return "config_saved_pending_reload";
                }
            };
            return match client.update_onebot_config(uin, payload).await {
                Ok(true) => "config_hot_reloaded",
                Ok(false) => "config_saved_pending_reload",
                Err(err) => {
                    tracing::warn!(error = ?err, "snowluma hot reload: update_onebot_config failed");
                    "config_saved_pending_reload"
                }
            };
        }

        // 远端:按 server_id 查共享 daemon。隧道可能因 SSH 断开被 drop_server 清掉
        // (tunnel_eps=None),此时用 daemon.current_host() 重开隧道;host 句柄若也被
        // poison,走 ensure_running_for_reconcile 路径会触发 daemon.ensure_running
        // 走通后才更新 tunnel_eps。拿到隧道端点后构造临时 client 调 update_onebot_config。
        let Some(server_id) = config.bot.runtime_target.server_id() else {
            return "config_saved_pending_reload";
        };
        if let Some((port, password, _)) = self
            .windows_bots
            .endpoint(server_id, config.bot.qq_id, BackendType::SnowLuma)
            .await
        {
            let Ok(client) = ReqwestSnowLumaWebUiClient::new(port, password) else {
                return "config_saved_pending_reload";
            };
            if client.login().await.is_err() {
                return "config_saved_pending_reload";
            }
            return match client.update_onebot_config(uin, payload).await {
                Ok(true) => "config_hot_reloaded",
                _ => "config_saved_pending_reload",
            };
        }
        let daemon = {
            let daemons = self.remote_snowluma_daemons.lock().await;
            daemons.get(server_id).cloned()
        };
        let Some(daemon) = daemon else {
            tracing::warn!(
                server_id,
                "snowluma hot reload: remote daemon not registered"
            );
            return "config_saved_pending_reload";
        };

        // tunnel_eps 缓存可能在 SSH 重连后被打成 None;ensure_running_for_reconcile 会
        // 用 daemon 当前 host(被 ServerManager replace 过的 live host)探测远端 node,
        // 在跑则重开隧道并把端点写回 tunnel_eps。node 不在就当离线,等下次重启生效。
        if daemon.tunnel_endpoints().await.is_none() {
            if let Err(err) = daemon.ensure_running_for_reconcile().await {
                tracing::warn!(
                    error = ?err,
                    server_id,
                    "snowluma hot reload: ensure_running_for_reconcile failed"
                );
                return "config_saved_pending_reload";
            }
        }
        let Some(eps) = daemon.tunnel_endpoints().await else {
            tracing::warn!(
                server_id,
                "snowluma hot reload: tunnel endpoints unavailable"
            );
            return "config_saved_pending_reload";
        };
        let client = match ReqwestSnowLumaWebUiClient::new(eps.webui_local_port, eps.webui_password)
        {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(error = ?err, "snowluma hot reload: build tunnel client failed");
                return "config_saved_pending_reload";
            }
        };
        // 远端临时 client 需先 login 拿 token,再 update
        if let Err(err) = client.login().await {
            tracing::warn!(error = ?err, "snowluma hot reload: tunnel client login failed");
            return "config_saved_pending_reload";
        }
        match client.update_onebot_config(uin, payload).await {
            Ok(true) => "config_hot_reloaded",
            Ok(false) => "config_saved_pending_reload",
            Err(err) => {
                tracing::warn!(error = ?err, "snowluma hot reload: tunnel update_onebot_config failed");
                "config_saved_pending_reload"
            }
        }
    }

    /// 把一份 OneBot 配置 payload 通过 NapCat WebUI 热推送给运行中的 bot
    ///
    /// 调用链:login(fetch_credential)→ check_login_status → set_ob11_config
    /// 任何一步失败都不阻塞保存——已经写盘了,最差就是等下次重启生效返回
    /// 一个 BotStateChanged 的 reason 字符串,让前端区分提示:
    /// - config_hot_reloaded:推送成功,配置已生效
    /// - config_saved_pending_login:QQ 还没扫码,待登录后下次启动生效
    /// - config_saved_pending_reload:网络 / 401 / 业务错误,等下次重启生效
    pub(super) async fn push_napcat_hot_reload(
        &self,
        endpoint: NapCatEndpoint,
        payload: serde_json::Value,
    ) -> &'static str {
        let NapCatEndpoint {
            port,
            token,
            host_port: _,
            online: _,
            auth_issue: _,
        } = endpoint;
        let credential = match self.webui_client.fetch_credential(port, &token).await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    port,
                    error = ?err,
                    "napcat hot reload: fetch_credential failed; saved without push"
                );
                return "config_saved_pending_reload";
            }
        };
        // QQ 未登录时 set_ob11_config 会返回 NotLogin;提前查一把可以让
        // 前端拿到更准确的语义(避免把"等扫码"误显示成"推送失败")
        match self
            .webui_client
            .check_login_status(port, &credential)
            .await
        {
            Ok(data) if !data.is_login && data.is_offline != Some(true) => {
                return "config_saved_pending_login";
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    port,
                    error = ?err,
                    "napcat hot reload: check_login_status failed; pushing anyway"
                );
            }
        }
        let body = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(error = ?err, "napcat hot reload: serialize payload failed");
                return "config_saved_pending_reload";
            }
        };
        match self
            .webui_client
            .set_ob11_config(port, &credential, &body)
            .await
        {
            Ok(_) => "config_hot_reloaded",
            Err(crate::napcat::webui_client::NapCatWebUiError::NotLogin) => {
                "config_saved_pending_login"
            }
            Err(err) => {
                tracing::warn!(port, error = ?err, "napcat hot reload: set_ob11_config failed");
                "config_saved_pending_reload"
            }
        }
    }
}
