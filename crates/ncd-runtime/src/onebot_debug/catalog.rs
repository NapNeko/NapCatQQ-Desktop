//! 动作目录：随包快照打底，Bot 在跑且上游有调试接口时换成现取的目录（快照补文档），
//! 再标注另一个后端有没有同名动作。
//!
//! 现取的目录按会话缓存，一个会话只取一次；取不到（没在跑、太老、连不上）就用快照，
//! 界面据 `source` 提示「按内置目录，可能与你的版本不同」。取失败后冷却一会儿再试，
//! 免得每次打开目录都重新登录一遍、再记一条告警。

use std::sync::{Arc, OnceLock};

use ncd_domain::bot_config::BackendType;
use ncd_domain::ids::BotId;
use ncd_domain::onebot_debug::{
    DebugActionSpec, DebugCatalog, DebugCatalogSource, DebugChannelStatus, DebugError,
};
use ncd_onebot::catalog::{Catalog, CatalogError, parse_napcat, parse_snowluma, snapshot};
use serde_json::Value;
use tokio::time::Instant;
use tracing::{debug, warn};

use super::calls::ChannelResult;
use super::port::DebugBotView;
use super::session::{CATALOG_RETRY_AFTER, INTERNAL_KEY};
use super::{DebugManager, Epoch};

/// 现取目录没有自己的版本号可展示（合并结果取快照的版本），给解析器一个占位
const LIVE_VERSION: &str = "live";

/// 两个后端的纯快照目录，各算一次
#[derive(Default)]
pub(super) struct SnapshotCatalogs {
    napcat: OnceLock<Arc<Catalog>>,
    snowluma: OnceLock<Arc<Catalog>>,
}

impl SnapshotCatalogs {
    fn get(&self, backend: BackendType) -> Arc<Catalog> {
        let cell = match backend {
            BackendType::NapCat => &self.napcat,
            BackendType::SnowLuma => &self.snowluma,
        };
        Arc::clone(cell.get_or_init(|| Arc::new(snapshot_only(backend))))
    }
}

impl DebugManager {
    pub async fn catalog(&self, bot_id: Option<&str>, backend: BackendType) -> DebugCatalog {
        self.catalog_for(bot_id, backend).await.to_debug_catalog()
    }

    pub async fn describe(
        &self,
        bot_id: Option<&str>,
        backend: BackendType,
        action: &str,
    ) -> Option<DebugActionSpec> {
        self.catalog_for(bot_id, backend).await.get(action).cloned()
    }

    async fn catalog_for(&self, bot_id: Option<&str>, backend: BackendType) -> Arc<Catalog> {
        if let Some(bot_id) = bot_id {
            if let Some(live) = self.live_catalog(&BotId::new(bot_id), backend).await {
                return live;
            }
        }
        self.snapshot_catalogs.get(backend)
    }

    /// 现取的目录；本会话取过就直接用缓存。任何取不到的情况都返回 None，由调用方退回快照
    async fn live_catalog(&self, bot_id: &BotId, backend: BackendType) -> Option<Arc<Catalog>> {
        let epoch = self.epoch();
        if !self.is_enabled() {
            return None;
        }
        let view = self.bots.bot(bot_id).await?;
        // 问的是另一个后端的目录，或者 Bot 没在跑：上游没法回答
        if view.config.bot.backend_type != backend || !view.running() {
            return None;
        }
        // 重启过、换过端点的 Bot 先忘掉之前的「太老」和失败冷却
        self.refresh_session(&view).await;
        {
            let sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(bot_id) {
                // 太老是确定的结论，等 Bot 重启或换了端点才重试；普通失败冷却一会儿再试
                if session.internal_too_old || session.catalog_cooling_down() {
                    return None;
                }
                if let Some(catalog) = &session.catalog {
                    return Some(Arc::clone(catalog));
                }
            }
        }

        let fetch = async {
            let client = match self.ensure_internal(&view, &epoch).await {
                Ok(client) => client,
                Err(failure) => {
                    // 多半是 WebUI 还没就绪（刚启动），很快就会好：不进冷却，也不算告警
                    debug!(bot_id = %bot_id, error = ?failure.error, "调试台：内部通道没就绪，动作目录改用内置快照");
                    return None;
                }
            };
            match self.fetch_catalog_data(&client).await {
                Ok(data) => {
                    let cached = self.cache_live_catalog(&view, &data, &epoch).await;
                    if cached.is_none() {
                        self.note_catalog_failure(&view, &epoch).await;
                    }
                    cached
                }
                Err(failure) if failure.error == DebugError::UpstreamTooOld => {
                    self.apply_status(
                        &view,
                        INTERNAL_KEY,
                        ChannelResult::Failed(DebugChannelStatus::UpstreamTooOld),
                        &epoch,
                    )
                    .await;
                    None
                }
                Err(failure) => {
                    warn!(
                        bot_id = %bot_id,
                        error = ?failure.error,
                        retry_after_secs = CATALOG_RETRY_AFTER.as_secs(),
                        "调试台：取上游动作目录失败，暂时改用内置快照"
                    );
                    self.note_catalog_failure(&view, &epoch).await;
                    None
                }
            }
        };
        tokio::select! {
            biased;
            _ = epoch.ended() => None,
            catalog = fetch => catalog,
        }
    }

    /// 解析上游给的目录并合并进快照，缓存到会话。解析失败记日志、不缓存；旧一轮的结果不缓存
    pub(super) async fn cache_live_catalog(
        &self,
        view: &DebugBotView,
        data: &Value,
        epoch: &Epoch,
    ) -> Option<Arc<Catalog>> {
        // 解析和合并是纯 CPU 活，在锁外做
        let catalog = match live_merged(view.config.bot.backend_type, data) {
            Ok(catalog) => Arc::new(catalog),
            Err(err) => {
                warn!(bot_id = %view.bot_id(), error = %err, "调试台：上游动作目录格式不对，改用内置快照");
                return None;
            }
        };
        let mut sessions = self.sessions.lock().await;
        let session = self.live_session(&mut sessions, view, epoch)?;
        session.catalog = Some(Arc::clone(&catalog));
        session.catalog_failed_at = None;
        Some(catalog)
    }

    async fn note_catalog_failure(&self, view: &DebugBotView, epoch: &Epoch) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = self.live_session(&mut sessions, view, epoch) {
            session.catalog_failed_at = Some(Instant::now());
        }
    }
}

fn other_backend(backend: BackendType) -> BackendType {
    match backend {
        BackendType::NapCat => BackendType::SnowLuma,
        BackendType::SnowLuma => BackendType::NapCat,
    }
}

/// 只有快照：整份标成快照来源，再对照另一个后端的快照
fn snapshot_only(backend: BackendType) -> Catalog {
    let mut catalog = Catalog::merge(None, snapshot(backend));
    catalog.annotate_other(snapshot(other_backend(backend)));
    catalog
}

/// 现取的目录为准、快照补文档，再对照另一个后端的快照
fn live_merged(backend: BackendType, data: &Value) -> Result<Catalog, CatalogError> {
    let live = match backend {
        BackendType::NapCat => parse_napcat(data, LIVE_VERSION, DebugCatalogSource::Live)?,
        BackendType::SnowLuma => parse_snowluma(data, LIVE_VERSION, DebugCatalogSource::Live)?,
    };
    let mut merged = Catalog::merge(Some(live), snapshot(backend));
    merged.annotate_other(snapshot(other_backend(backend)));
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn snapshot_catalog_is_marked_snapshot_and_annotated() {
        for backend in [BackendType::NapCat, BackendType::SnowLuma] {
            let catalog = snapshot_only(backend);
            assert_eq!(catalog.source(), DebugCatalogSource::Snapshot);
            assert!(catalog.actions().len() >= 150, "{backend:?}");
            assert!(
                catalog
                    .actions()
                    .iter()
                    .all(|a| a.other_backend.as_ref().map(|o| o.backend)
                        == Some(other_backend(backend))),
                "{backend:?} 每个动作都应标注另一个后端"
            );
        }
    }

    #[test]
    fn live_napcat_catalog_wins_and_keeps_snapshot_extras_as_unsupported() {
        let data = json!({
            "get_login_info": {"description": "现取的说明", "payload": {"type": "object", "properties": {}}},
        });
        let merged = live_merged(BackendType::NapCat, &data).unwrap();
        assert_eq!(merged.source(), DebugCatalogSource::Live);
        let login = merged.get("get_login_info").unwrap();
        assert!(login.supported);
        assert_eq!(login.source, DebugCatalogSource::Live);
        // 快照里有、现取的没有：保留文档但标不支持
        let missing = merged
            .actions()
            .iter()
            .find(|a| a.name != "get_login_info")
            .unwrap();
        assert!(!missing.supported);
        assert!(login.other_backend.is_some());
    }

    #[test]
    fn broken_live_catalog_is_an_error() {
        assert!(live_merged(BackendType::SnowLuma, &json!({"nope": 1})).is_err());
        assert!(live_merged(BackendType::NapCat, &json!([1, 2])).is_err());
    }
}
