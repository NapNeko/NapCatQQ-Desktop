//! 跑着只打 Dashboard API：先 GET 全量再 merge，禁止浅覆盖整节。

use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde_json::Value;

use super::ai::{self, apply_ai_patch, restore_extras};
use super::config::{
    AstrBotInstanceConfig, apply_onebot_to_root, claimed_platform_row, read_astrbot_config,
};
use super::config_json::{cmd_config_path, parse_cmd_config};
use super::dashboard_client::DashboardClient;
use super::platform::row_id;
use crate::config_doc::DocumentSnapshot;

pub struct LiveTarget {
    pub instance_id: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub conf_id: String,
}

pub async fn write_live(
    host: &dyn Host,
    install_dir: &HostPath,
    instance_id: &str,
    listen_port: u16,
    cfg: &AstrBotInstanceConfig,
    target: &LiveTarget,
) -> Result<(AstrBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = cfg.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let client = DashboardClient::connect(&target.instance_id, "127.0.0.1", target.port)?;
    client.login(&target.username, &target.password).await?;

    let conf_id = if target.conf_id.trim().is_empty() {
        "default"
    } else {
        target.conf_id.trim()
    };

    let payload = client.get_config().await?;
    let mut live = payload
        .get("config")
        .cloned()
        .ok_or_else(|| AppFrameworkError::Integration("Dashboard 未返回 config".into()))?;

    apply_onebot_to_root(&mut live, instance_id, cfg)?;
    let mut sources = cfg.sources.clone();
    let mut models = cfg.models.clone();
    restore_extras(&live, &mut sources, &mut models);
    apply_ai_patch(
        &mut live,
        &sources,
        &models,
        &cfg.ai,
        &cfg.stt,
        &cfg.tts,
        &cfg.websearch,
        &cfg.kb,
        &cfg.gates,
        &cfg.subagent,
    )
    .map_err(AppFrameworkError::Integration)?;

    if conf_id == "default" {
        sync_providers(&client, &payload, &sources, &models).await?;
        sync_claimed_platform(&client, &payload, instance_id, listen_port, cfg, &live).await?;
    }

    // default 的 update 会丢掉 provider / platform；AI 组单独 POST 全量 merge 对象
    client.update_astrbot_config(conf_id, &live).await?;
    // subagent 那个接口只认默认配置：非 default 走它会把当前 profile 的编排写进 default
    if conf_id == "default" {
        if let Some(body) = live.get("subagent_orchestrator") {
            super::runtime::write_subagent(&client, body).await?;
        }
    }

    // API 会写盘；再读文件拿新 revision
    let path = cmd_config_path(install_dir);
    let bytes = host
        .read_file(&path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    let _ = parse_cmd_config(&String::from_utf8_lossy(&bytes))?;
    read_astrbot_config(host, install_dir, instance_id, listen_port).await
}

async fn sync_claimed_platform(
    client: &DashboardClient,
    payload: &Value,
    instance_id: &str,
    listen_port: u16,
    _cfg: &AstrBotInstanceConfig,
    live: &Value,
) -> Result<(), AppFrameworkError> {
    let before = payload.get("config").unwrap_or(payload);
    let before_row = claimed_platform_row(before, instance_id, listen_port);
    let after_row = claimed_platform_row(live, instance_id, listen_port)
        .cloned()
        .ok_or_else(|| AppFrameworkError::Integration("认领的 OneBot 行不存在".into()))?;
    match before_row.and_then(row_id) {
        Some(id) => client.platform_update(id, &after_row).await,
        None => client.platform_new(&after_row).await,
    }
}

async fn sync_providers(
    client: &DashboardClient,
    payload: &Value,
    sources: &[ai::AstrBotProviderSource],
    models: &[ai::AstrBotProviderModel],
) -> Result<(), AppFrameworkError> {
    let before = payload.get("config").unwrap_or(payload);
    let old_sources = ai::sources_from_root(before);
    let old_models = ai::models_from_root(before);

    // 先建后删：删源会连带删掉它下面的模型，先删就把用户刚挪过去的模型也带走了
    for src in sources {
        client
            .provider_source_upsert(&src.id, &ai::source_to_value(src))
            .await?;
    }
    let old_model_ids: std::collections::HashSet<&str> =
        old_models.iter().map(|m| m.id.as_str()).collect();
    for row in models {
        let val = ai::model_to_value(row);
        if old_model_ids.contains(row.id.as_str()) {
            client.provider_update(&row.id, &val).await?;
        } else {
            client.provider_new(&val).await?;
        }
    }

    let new_model_ids: std::collections::HashSet<&str> =
        models.iter().map(|m| m.id.as_str()).collect();
    for old in &old_models {
        if !new_model_ids.contains(old.id.as_str()) {
            client.provider_delete(&old.id).await?;
        }
    }
    let new_source_ids: std::collections::HashSet<&str> =
        sources.iter().map(|s| s.id.as_str()).collect();
    for old in &old_sources {
        if !new_source_ids.contains(old.id.as_str()) {
            client.provider_source_delete(&old.id).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use super::super::dashboard_client::clear_token;

    async fn logged_in(server: &MockServer, instance_id: &str) -> DashboardClient {
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt" }
            })))
            .mount(server)
            .await;
        for p in [
            "/api/config/provider_sources/update",
            "/api/config/provider_sources/delete",
            "/api/config/provider/new",
            "/api/config/provider/update",
            "/api/config/provider/delete",
        ] {
            Mock::given(method("POST"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "ok" })))
                .mount(server)
                .await;
        }
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        clear_token(instance_id);
        let client = DashboardClient::connect(instance_id, "127.0.0.1", port).unwrap();
        client.login("astrbot", "Abcdefg1").await.unwrap();
        client
    }

    async fn write_paths(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.url.path().to_string())
            .filter(|p| p != "/api/auth/login")
            .collect()
    }

    #[tokio::test]
    async fn a_brand_new_source_goes_through_update_because_there_is_no_new_endpoint() {
        let server = MockServer::start().await;
        let client = logged_in(&server, "live-new-src").await;
        let payload = json!({ "config": { "provider_sources": [], "provider": [] } });
        let src = ai::AstrBotProviderSource {
            id: "s1".into(),
            ..Default::default()
        };

        sync_providers(&client, &payload, &[src], &[]).await.unwrap();

        assert_eq!(
            write_paths(&server).await,
            ["/api/config/provider_sources/update"]
        );
    }

    #[tokio::test]
    async fn removed_source_is_deleted_only_after_everything_is_written() {
        let server = MockServer::start().await;
        let client = logged_in(&server, "live-order").await;
        let payload = json!({
            "config": {
                "provider_sources": [{ "id": "old", "provider": "openai" }],
                "provider": [{ "id": "m-old", "provider_source_id": "old", "model": "gpt-4o" }]
            }
        });
        let src = ai::AstrBotProviderSource {
            id: "new".into(),
            ..Default::default()
        };
        let model = ai::AstrBotProviderModel {
            id: "m-new".into(),
            provider_source_id: "new".into(),
            model: "gpt-4o".into(),
            ..Default::default()
        };

        sync_providers(&client, &payload, &[src], &[model])
            .await
            .unwrap();

        // 删源会连带删掉它下面的模型，所以两个删都得排在两个写后面
        assert_eq!(
            write_paths(&server).await,
            [
                "/api/config/provider_sources/update",
                "/api/config/provider/new",
                "/api/config/provider/delete",
                "/api/config/provider_sources/delete",
            ]
        );
    }
}
