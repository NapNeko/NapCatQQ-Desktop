//! 只从官方目录拉商店；UI 禁止自己 fetch。
//!
//! NoneBot 对齐 nb-cli `download_module_data`：多源竞速 + 进程内缓存。
//! 主源 `registry.nonebot.dev/{adapters,plugins}.json`，镜像走 registry `results` 分支。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ncd_appframework::{
    AppStoreMarketEntry, KarinPluginMarketEntry, parse_karin_plugins_list,
    parse_nonebot_adapters_json, parse_nonebot_plugins_json,
};
use ncd_domain::AppStoreResource;
use ncd_network::shared_client;
use ncd_traits::AppFrameworkError;
use tokio::task::JoinSet;

pub const KARIN_PLUGINS_LIST_URL: &str = "https://registry.npmjs.com/@karinjs/plugins-list/latest";
const NONEBOT_ADAPTERS_URL: &str = "https://registry.nonebot.dev/adapters.json";
const NONEBOT_PLUGINS_URL: &str = "https://registry.nonebot.dev/plugins.json";

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

/// nb-cli 同序：官网 → jsDelivr → 国内 jsDelivr → gh-proxy。
pub fn nonebot_registry_urls(file: &str) -> Vec<String> {
    let official = match file {
        "adapters.json" => NONEBOT_ADAPTERS_URL.to_string(),
        "plugins.json" => NONEBOT_PLUGINS_URL.to_string(),
        other => format!("https://registry.nonebot.dev/{other}"),
    };
    vec![
        official,
        format!("https://cdn.jsdelivr.net/gh/nonebot/registry@results/{file}"),
        format!("https://jsd.cdn.zzko.cn/gh/nonebot/registry@results/{file}"),
        format!(
            "https://gh-proxy.com/https://raw.githubusercontent.com/nonebot/registry/results/{file}"
        ),
    ]
}

pub async fn fetch_karin_plugin_market() -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    fetch_karin_plugin_market_from(KARIN_PLUGINS_LIST_URL).await
}

pub async fn fetch_karin_plugin_market_from(
    url: &str,
) -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    let text = fetch_text(url, "插件目录").await?;
    parse_karin_plugins_list(&text)
}

pub async fn fetch_store(
    framework_id: &str,
    resource: AppStoreResource,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    match (framework_id, resource) {
        ("karin", AppStoreResource::Plugin) => Ok(fetch_karin_plugin_market()
            .await?
            .into_iter()
            .map(AppStoreMarketEntry::from_karin)
            .collect()),
        ("karin", AppStoreResource::Adapter) => Ok(Vec::new()),
        ("nonebot2", AppStoreResource::Adapter) => fetch_nonebot_adapters().await,
        ("nonebot2", AppStoreResource::Plugin) => fetch_nonebot_plugins().await,
        (other, _) => Err(AppFrameworkError::PluginUnsupported(other.to_string())),
    }
}

pub async fn fetch_nonebot_adapters() -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    if let Some(hit) = cache_get("adapters") {
        return Ok(hit);
    }
    let text = fetch_text_first_ok(&nonebot_registry_urls("adapters.json"), "适配器目录").await?;
    let list = parse_nonebot_adapters_json(&text)?;
    cache_put("adapters", list.clone());
    Ok(list)
}

pub async fn fetch_nonebot_plugins() -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    if let Some(hit) = cache_get("plugins") {
        return Ok(hit);
    }
    let text = fetch_text_first_ok(&nonebot_registry_urls("plugins.json"), "插件目录").await?;
    let list = parse_nonebot_plugins_json(&text)?;
    cache_put("plugins", list.clone());
    Ok(list)
}

#[cfg(test)]
async fn fetch_nonebot_adapters_from(
    url: &str,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let text = fetch_text(url, "适配器目录").await?;
    parse_nonebot_adapters_json(&text)
}

#[cfg(test)]
async fn fetch_nonebot_plugins_from(
    url: &str,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let text = fetch_text(url, "插件目录").await?;
    parse_nonebot_plugins_json(&text)
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
        let list = fetch_karin_plugin_market_from(&format!("{}/latest", server.uri()))
            .await
            .unwrap();
        assert_eq!(list[0].name, "x");
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
        let list = fetch_nonebot_adapters_from(&server.uri()).await.unwrap();
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
        let list = fetch_nonebot_plugins_from(&server.uri()).await.unwrap();
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
