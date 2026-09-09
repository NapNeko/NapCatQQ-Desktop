//! 只从官方目录拉商店；UI 禁止自己 fetch。
//!
//! NoneBot 对齐 nb-cli `download_module_data`：多源竞速 + 进程内缓存。
//! 主源 `registry.nonebot.dev/{adapters,plugins}.json`，镜像走 registry `results` 分支。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ncd_appframework::{AppFrameworkRegistry, AppStoreMarketEntry, KarinPluginMarketEntry};
use ncd_domain::AppFrameworkId;
use ncd_domain::AppStoreResource;
use ncd_network::shared_client;
use ncd_traits::AppFrameworkError;
use tokio::task::JoinSet;

pub use ncd_appframework::KARIN_PLUGINS_LIST_URL;

const MARKET_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

struct CachedMarket {
    at: Instant,
    entries: Vec<AppStoreMarketEntry>,
}

fn market_cache() -> &'static Mutex<HashMap<&'static str, CachedMarket>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, CachedMarket>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_get(key: &'static str) -> Option<Vec<AppStoreMarketEntry>> {
    let guard = market_cache().lock().ok()?;
    let hit = guard.get(key)?;
    (hit.at.elapsed() < MARKET_CACHE_TTL).then(|| hit.entries.clone())
}

fn cache_put(key: &'static str, entries: Vec<AppStoreMarketEntry>) {
    if let Ok(mut guard) = market_cache().lock() {
        guard.insert(
            key,
            CachedMarket {
                at: Instant::now(),
                entries,
            },
        );
    }
}

pub async fn fetch_karin_plugin_market() -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    Ok(fetch_store("karin", AppStoreResource::Plugin)
        .await?
        .into_iter()
        .filter_map(|e| e.to_karin())
        .collect())
}

pub async fn fetch_store(
    framework_id: &str,
    resource: AppStoreResource,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let adapter = match AppFrameworkRegistry::with_builtin().get(&AppFrameworkId::new(framework_id)) {
        Ok(adapter) => adapter,
        Err(AppFrameworkError::NotRegistered(id)) => {
            return Err(AppFrameworkError::PluginUnsupported(id));
        }
        Err(err) => return Err(err),
    };
    if let Some(key) = adapter.store_market_cache_key(resource) {
        if let Some(hit) = cache_get(key) {
            return Ok(hit);
        }
    }
    let urls = adapter.store_market_urls(resource);
    if urls.is_empty() {
        return Ok(Vec::new());
    }
    let label = match resource {
        AppStoreResource::Adapter => "适配器目录",
        AppStoreResource::Plugin => "插件目录",
    };
    let text = if urls.len() == 1 {
        fetch_text(&urls[0], label).await?
    } else {
        fetch_text_first_ok(&urls, label).await?
    };
    let list = adapter.parse_store_market(resource, &text)?;
    if let Some(key) = adapter.store_market_cache_key(resource) {
        cache_put(key, list.clone());
    }
    Ok(list)
}

const MARKET_BODY_LIMIT: usize = 8 * 1024 * 1024;

async fn fetch_text_first_ok(urls: &[String], label: &str) -> Result<String, AppFrameworkError> {
    if urls.is_empty() {
        return Err(AppFrameworkError::Validation(format!("拉取{label}失败: 没有可用源")));
    }
    let mut set = JoinSet::new();
    for url in urls {
        let url = url.clone();
        let label = label.to_string();
        set.spawn(async move { fetch_text(&url, &label).await });
    }
    let mut last_err = None;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(text)) => {
                set.abort_all();
                return Ok(text);
            }
            Ok(Err(err)) => last_err = Some(err),
            Err(err) => {
                last_err = Some(AppFrameworkError::Validation(format!(
                    "拉取{label}失败: {err}"
                )));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        AppFrameworkError::Validation(format!("拉取{label}失败: 所有源都不可用"))
    }))
}

async fn fetch_text(url: &str, label: &str) -> Result<String, AppFrameworkError> {
    let resp = shared_client()
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| AppFrameworkError::Validation(format!("拉取{label}失败: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppFrameworkError::Validation(format!(
            "拉取{label}失败: HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppFrameworkError::Validation(format!("读取{label}失败: {e}")))?;
    if bytes.len() > MARKET_BODY_LIMIT {
        return Err(AppFrameworkError::Validation(format!(
            "读取{label}失败: 目录过大"
        )));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_appframework::{
        KarinAdapter, NoneBot2Adapter, NONEBOT_ADAPTERS_URL, NONEBOT_PLUGINS_URL,
        nonebot_registry_urls,
    };

    async fn parse_market_from(
        adapter: &dyn ncd_appframework::AppFrameworkAdapter,
        resource: AppStoreResource,
        url: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        let label = match resource {
            AppStoreResource::Adapter => "适配器目录",
            AppStoreResource::Plugin => "插件目录",
        };
        let text = fetch_text(url, label).await?;
        adapter.parse_store_market(resource, &text)
    }

    #[tokio::test]
    async fn fetch_market_reads_plugins_array() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_string(
                    r#"{"plugins":[{"name":"x","type":"npm","description":"d","time":"2025-01-01 00:00:00","home":"h","author":[],"repo":[]}]}"#,
                ),
            )
            .mount(&server)
            .await;
        let list = parse_market_from(
            &KarinAdapter::new(),
            AppStoreResource::Plugin,
            &format!("{}/latest", server.uri()),
        )
        .await
        .unwrap();
        assert_eq!(list[0].id, "x");
    }

    #[tokio::test]
    async fn fetch_nonebot_adapters_reads_array() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                r#"[{"module_name":"nonebot.adapters.console","project_link":"nonebot-adapter-console","name":"Console"}]"#,
            ))
            .mount(&server)
            .await;
        let list = parse_market_from(
            &NoneBot2Adapter::new(),
            AppStoreResource::Adapter,
            &server.uri(),
        )
        .await
        .unwrap();
        assert_eq!(list[0].id, "nonebot.adapters.console");
        assert_eq!(list[0].resource, AppStoreResource::Adapter);
    }

    #[test]
    fn nonebot_registry_urls_follow_nb_cli() {
        let plugins = nonebot_registry_urls("plugins.json");
        let adapters = nonebot_registry_urls("adapters.json");
        assert_eq!(plugins[0], NONEBOT_PLUGINS_URL);
        assert_eq!(adapters[0], NONEBOT_ADAPTERS_URL);
        assert!(plugins.iter().any(|u| u.contains("jsdelivr.net/gh/nonebot/registry@results")));
        assert!(plugins.iter().any(|u| u.contains("raw.githubusercontent.com/nonebot/registry/results")));
    }

    #[tokio::test]
    async fn fetch_nonebot_plugins_reads_array() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                r#"[{"module_name":"nonebot_plugin_foo","project_link":"nonebot-plugin-foo","name":"Foo","valid":true}]"#,
            ))
            .mount(&server)
            .await;
        let list = parse_market_from(
            &NoneBot2Adapter::new(),
            AppStoreResource::Plugin,
            &server.uri(),
        )
        .await
        .unwrap();
        assert_eq!(list[0].id, "nonebot_plugin_foo");
        assert_eq!(list[0].resource, AppStoreResource::Plugin);
    }

    #[tokio::test]
    async fn fetch_text_first_ok_skips_failed_mirror() {
        let bad = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(503))
            .mount(&bad)
            .await;
        let good = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&good)
            .await;
        let text = fetch_text_first_ok(
            &[bad.uri(), good.uri()],
            "适配器目录",
        )
        .await
        .unwrap();
        assert_eq!(text, "[]");
    }
}
