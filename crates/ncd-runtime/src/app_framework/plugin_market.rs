//! 只从官方 `@karinjs/plugins-list` 拉目录；UI 禁止自己 fetch。

use ncd_appframework::{KarinPluginMarketEntry, parse_karin_plugins_list};
use ncd_network::shared_client;
use ncd_traits::AppFrameworkError;

pub const KARIN_PLUGINS_LIST_URL: &str = "https://registry.npmjs.com/@karinjs/plugins-list/latest";

pub async fn fetch_karin_plugin_market() -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    fetch_karin_plugin_market_from(KARIN_PLUGINS_LIST_URL).await
}

pub async fn fetch_karin_plugin_market_from(
    url: &str,
) -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    let resp = shared_client()
        .get(url)
        .send()
        .await
        .map_err(|e| AppFrameworkError::Validation(format!("拉取插件目录失败: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppFrameworkError::Validation(format!(
            "拉取插件目录失败: HTTP {}",
            resp.status()
        )));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| AppFrameworkError::Validation(format!("读取插件目录失败: {e}")))?;
    parse_karin_plugins_list(&text)
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
}
