//! Bot 快照、配置、日志和 WebUI 端点的只读查询。

use super::{BotManager, BotManagerError};
use crate::bot_actor::BotActorSnapshot;
use ncd_domain::bot_config::BotConfig;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::RuntimeTarget;
use ncd_traits::{BotConfigRepo, ConfigStore};
use std::collections::HashMap;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    // ─── 查询 ─────────────────────────────────────────────────────────────

    /// 获取所有 Bot 的当前快照（按 bot_id 稳定升序）
    pub async fn list_snapshots(&self) -> Vec<BotActorSnapshot> {
        let actors = self.actors.read().await;
        let mut list: Vec<BotActorSnapshot> = actors.values().map(|h| h.snapshot()).collect();
        list.sort_by(|a, b| a.bot_id.cmp(&b.bot_id));
        list
    }

    /// 获取指定 Bot 的当前快照
    pub async fn get_snapshot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        Ok(handle.snapshot())
    }

    /// 获取指定 Bot 的当前配置
    pub async fn get_bot_config(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<BotConfig>, BotManagerError> {
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        self.repo.get(qq_id).await.map_err(BotManagerError::from)
    }

    /// 列出全部 Bot 配置(持久化源)
    pub async fn list_bot_configs(&self) -> Result<Vec<BotConfig>, BotManagerError> {
        self.repo.list().await.map_err(BotManagerError::from)
    }

    /// 供 ncd-watch 同步:优先内存 endpoint(真实 port/token),
    /// Docker 再回退 secret store 中的 token,端口为 0 表示未知。
    /// 不创建新 secret,避免 watch 同步副作用写盘。
    pub async fn napcat_webui_for_watch(&self, bot_id: &BotId) -> Option<(u16, String)> {
        if let Some(ep) = self.napcat_endpoints.snapshot(bot_id).await {
            let port = ep.watch_port();
            if port > 0 && !ep.token.trim().is_empty() {
                return Some((port, ep.token));
            }
        }
        // 会话还没开起来就不知道容器实际映射到哪个口:只给 token,端口填 0,
        // 由 notify 组装按规则推并标成推测值,不盖掉远端 notify.json 里已有的口
        let qq: u64 = bot_id.as_str().parse().ok()?;
        let token = self.peek_napcat_docker_webui_token(qq)?;
        Some((0, token))
    }

    /// 列出当前内存中的 NapCat WebUI 端点(Desktop 本机可达 port + token)
    /// 冷启动 / 页面刷新后前端 hydrate 用;多实例时 port 可能是 6099/6100/...
    pub async fn list_napcat_webui_endpoints(&self) -> Vec<(BotId, u16, String, Option<bool>)> {
        self.napcat_endpoints
            .list_all()
            .await
            .into_iter()
            .filter(|(_, ep)| ep.port > 0 && !ep.token.trim().is_empty())
            .map(|(id, ep)| (id, ep.port, ep.token, ep.online))
            .collect()
    }

    /// 当前换不到 WebUI 凭据的 Bot 及原因;和 list_napcat_webui_endpoints 一起给前端回填
    pub async fn list_napcat_webui_auth_issues(
        &self,
    ) -> HashMap<BotId, ncd_domain::NapCatWebuiAuthIssue> {
        self.napcat_endpoints
            .list_all()
            .await
            .into_iter()
            .filter_map(|(id, ep)| ep.auth_issue.map(|issue| (id, issue)))
            .collect()
    }

    /// SnowLuma UI 会话态快照(daemon + per-bot 登录/隧道),冷启动 hydrate 用
    pub async fn list_snowluma_ui_snapshot(&self) -> crate::snowluma_ui_state::SnowLumaUiSnapshot {
        self.snowluma_ui.snapshot().await
    }

    fn peek_napcat_docker_webui_token(&self, qq_id: u64) -> Option<String> {
        let store = self.docker_webui_secret_store.as_ref()?;
        let key = format!("bot:{qq_id}:napcat_docker_webui_token");
        store
            .get(&key)
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// 批量返回所有 Bot 的 backend_type,用于 UI 列表页一次性拿 flavor map
    /// 避免 BotListPage 对每个 bot 单独调 get_bot_config 造成 N+1
    /// key 为 BotId.to_string()(即 QQID 数字字符串)
    pub async fn list_bot_flavors(
        &self,
    ) -> Result<
        std::collections::HashMap<String, ncd_domain::bot_config::BackendType>,
        BotManagerError,
    > {
        let configs = self.repo.list().await?;
        let mut out = std::collections::HashMap::with_capacity(configs.len());
        for cfg in configs {
            out.insert(cfg.bot.qq_id.to_string(), cfg.bot.backend_type);
        }
        Ok(out)
    }

    /// 当前托管的 Bot 数量
    pub async fn bot_count(&self) -> usize {
        self.actors.read().await.len()
    }

    /// 当前活跃(Starting / Running / Stopping)的 Bot 数量
    pub async fn active_count(&self) -> usize {
        let actors = self.actors.read().await;
        actors
            .values()
            .filter(|h| h.snapshot().state.is_active())
            .count()
    }

    /// 本机 runtime_target 且处于活跃态的 Bot 数量(退出拦截用)
    pub async fn count_local_active_bots(&self) -> Result<usize, BotManagerError> {
        self.count_active_bots_by_host(|t| t.is_local()).await
    }

    /// 远端 runtime_target 且处于活跃态的 Bot 数量(退出提示用,不拦截退出)
    pub async fn count_remote_active_bots(&self) -> Result<usize, BotManagerError> {
        self.count_active_bots_by_host(|t| !t.is_local()).await
    }

    /// 组件页 host_id(`local` / `remote:{server_id}`)上活跃 Bot 数
    /// Starting / Running / Stopping 均算活跃;与组件 update/uninstall 门禁一致
    pub async fn count_active_bots_on_component_host(
        &self,
        host_id: &str,
    ) -> Result<usize, BotManagerError> {
        let host_id = host_id.trim();
        if host_id.is_empty() {
            return Ok(0);
        }
        if host_id == "local" {
            return self.count_local_active_bots().await;
        }
        let Some(server_id) = host_id.strip_prefix("remote:") else {
            // 未知 host 形态不按本机兜底,避免误拦/误放
            return Ok(0);
        };
        if server_id.is_empty() {
            return Ok(0);
        }
        self.count_active_bots_by_host(|t| t.server_id() == Some(server_id))
            .await
    }

    async fn count_active_bots_by_host(
        &self,
        host_match: impl Fn(&RuntimeTarget) -> bool,
    ) -> Result<usize, BotManagerError> {
        let configs = self.repo.list().await?;
        // bot_id 字符串 → runtime_target,避免对每个 snapshot 线性扫 configs 并 to_string
        let target_by_bot_id: HashMap<String, &RuntimeTarget> = configs
            .iter()
            .map(|c| (c.bot.qq_id.to_string(), &c.bot.runtime_target))
            .collect();
        let snapshots = self.list_snapshots().await;
        Ok(snapshots
            .iter()
            .filter(|s| s.state.is_active())
            .filter(|s| {
                target_by_bot_id
                    .get(s.bot_id.as_str())
                    .is_some_and(|t| host_match(t))
            })
            .count())
    }

    /// 拉取指定 Bot 的最近 lines 行日志快照
    /// 返回 [LogSnapshot],包含已截尾的日志行 + 总行数供 UI 在 BotLogPage
    /// 初次开页时一次性加载历史,再叠加 bot_log_appended / snowluma_daemon_log
    /// 实时事件对齐 legacy NapCatQQProcessLog.get_log_content 行为:本地是
    /// 内存 deque 快照(进程存活期间累计的全量),进程被 stop / 重启时缓冲清零
    ///
    /// 必须按 bot 当前配置的 backend 路由,不能写死走默认 NapCat backend:
    /// 否则 SnowLuma flavor 的 bot 会去 NapCat backend 拉历史,拿到的是磁盘
    /// 归档里 NC 旧日志,配置切换 NC → SL 后用户看到的依然是 NC 的内容
    pub async fn tail_log(
        &self,
        bot_id: &BotId,
        lines: usize,
    ) -> Result<ncd_traits::runtime_backend::LogSnapshot, BotManagerError> {
        // Actor 不存在直接返回空,UI 不需要为此报错
        if !self.actors.read().await.contains_key(bot_id) {
            return Ok(ncd_traits::runtime_backend::LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            });
        }
        let opts = ncd_traits::runtime_backend::TailOpts { lines };
        match self.get_bot_config(bot_id).await? {
            Some(cfg) => {
                let backend = self.backend_for_lifecycle(&cfg).await?;
                backend
                    .tail_log(bot_id.clone(), opts)
                    .await
                    .map_err(BotManagerError::from)
            }
            None => Ok(ncd_traits::runtime_backend::LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            }),
        }
    }

    /// 远端 SnowLuma Docker 隧道端口(供 Tauri open_snowluma_webui)
    pub async fn snowluma_docker_endpoints(
        &self,
        bot_id: &BotId,
    ) -> Option<crate::docker_bot_session::SnowLumaDockerEndpoints> {
        self.remote_runtime_sessions()
            .snowluma_docker_endpoints(bot_id)
            .await
    }

    /// 远端 SnowLuma Native:按 SSH server_id 查本机隧道端口(多 Bot 同机共享)
    pub async fn snowluma_native_endpoints_for_server(
        &self,
        server_id: &str,
    ) -> Option<ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelEndpoints> {
        if let Some((port, password, _)) = self.windows_bots.snowluma_endpoint(server_id).await {
            return Some(
                ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelEndpoints {
                    webui_local_port: port,
                    webui_password: password,
                    novnc_local_port: 0,
                    vnc_password: String::new(),
                },
            );
        }
        let daemon = self
            .remote_snowluma_daemons
            .lock()
            .await
            .get(server_id)
            .cloned();
        if let Some(daemon) = daemon {
            if let Some(endpoint) = daemon.tunnel_endpoints().await {
                return Some(endpoint);
            }
        }
        self.remote_snowluma_tunnels
            .endpoints_for_server(server_id)
            .await
    }
}
