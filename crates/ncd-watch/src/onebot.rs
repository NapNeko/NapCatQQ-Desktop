//! 同机 OneBot HTTP 投递(对齐 Desktop notify/onebot,source=watch)
//!
//! Docker messenger 的宿主机口以 docker inspect 现查为准:导入的容器不一定按桌面端的偏移
//! 规则映射,notify.json 里的 base_url 只是回退。一个 messenger 整体发不出去就换下一个。

use std::collections::HashSet;
use std::process::Stdio;

use ncd_domain::docker::{DOCKER_INSPECT_PORTS_FORMAT, parse_published_host_port};
use ncd_domain::{OfflineAlert, render_template};

use crate::config::{WatchOneBotMessenger, WatchOneBotSettings};

const TIMEOUT_SECS: u64 = 10;

/// 查容器端口在宿主机上的实际映射;测试里换成假的
pub trait PortResolver: Sync {
    fn published_port(&self, container: &str, container_port: u16) -> Option<u16>;
}

/// 本机 docker CLI;docker 不可用或容器不存在都当查不到,交给 base_url 回退
pub struct DockerPortResolver;

impl PortResolver for DockerPortResolver {
    fn published_port(&self, container: &str, container_port: u16) -> Option<u16> {
        let out = std::process::Command::new("docker")
            .args([
                "inspect",
                "--format",
                DOCKER_INSPECT_PORTS_FORMAT,
                container,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_published_host_port(&String::from_utf8_lossy(&out.stdout), container_port)
            .ok()
            .flatten()
    }
}

/// 按配置顺序列出这次能试的 messenger:不是掉线的 bot;给了在线集合时还要求本轮进程在线
pub fn candidate_messengers<'a>(
    settings: &'a WatchOneBotSettings,
    offline_bot_id: &str,
    online_bot_ids: Option<&HashSet<String>>,
) -> Vec<&'a WatchOneBotMessenger> {
    if !settings.enabled {
        return Vec::new();
    }
    settings
        .messengers
        .iter()
        .filter(|m| {
            let id = m.bot_id.trim();
            if id.is_empty() || id == offline_bot_id {
                return false;
            }
            online_bot_ids.is_none_or(|online| online.contains(id))
        })
        .collect()
}

/// 这个 messenger 这次该连的 OneBot HTTP 根地址;两条路都没有就返回 None
pub fn resolve_base_url(m: &WatchOneBotMessenger, resolver: &dyn PortResolver) -> Option<String> {
    let container = m
        .container_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let port = m.container_port.filter(|p| *p > 0);
    if let (Some(container), Some(port)) = (container, port) {
        if let Some(host_port) = resolver.published_port(container, port) {
            return Some(format!("http://127.0.0.1:{host_port}"));
        }
    }
    let url = m.base_url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

pub async fn send_watch_onebot(
    settings: &WatchOneBotSettings,
    offline_bot_id: &str,
    online_bot_ids: Option<&HashSet<String>>,
    alert: &OfflineAlert,
) -> Result<(), String> {
    send_watch_onebot_with(
        settings,
        offline_bot_id,
        online_bot_ids,
        alert,
        &DockerPortResolver,
    )
    .await
}

pub async fn send_watch_onebot_with(
    settings: &WatchOneBotSettings,
    offline_bot_id: &str,
    online_bot_ids: Option<&HashSet<String>>,
    alert: &OfflineAlert,
    resolver: &dyn PortResolver,
) -> Result<(), String> {
    if !settings.enabled {
        return Ok(());
    }
    let targets: Vec<u64> = settings
        .target_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect();
    if targets.is_empty() {
        return Err("OneBot 目标 ID 未配置".into());
    }

    let mut errors = Vec::new();
    for m in candidate_messengers(settings, offline_bot_id, online_bot_ids) {
        let Some(base_url) = resolve_base_url(m, resolver) else {
            errors.push(format!("{}: 没有可连的 OneBot HTTP 地址", m.bot_id));
            continue;
        };
        match send_with_messenger(&base_url, &m.access_token, settings, alert, &targets).await {
            Ok(()) => return Ok(()),
            // 已经有目标收到了,换 messenger 会让这些人再收一遍
            Err(SendError::Partial(e)) => return Err(e),
            Err(SendError::AllFailed(e)) => errors.push(format!("{}: {e}", m.bot_id)),
        }
    }
    if errors.is_empty() {
        return Err("无可用同机 OneBot messenger".into());
    }
    Err(format!("OneBot 发送失败: {}", errors.join("; ")))
}

enum SendError {
    AllFailed(String),
    Partial(String),
}

async fn send_with_messenger(
    base_url: &str,
    access_token: &str,
    settings: &WatchOneBotSettings,
    alert: &OfflineAlert,
    targets: &[u64],
) -> Result<(), SendError> {
    let mut vars = alert.template_vars();
    vars.push(("source", "watch".into()));
    let text = render_template(&settings.message_template, &vars);
    let is_group = settings.target_type.eq_ignore_ascii_case("group");
    let path = if is_group {
        "/send_group_msg"
    } else {
        "/send_private_msg"
    };
    let id_key = if is_group { "group_id" } else { "user_id" };
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| SendError::AllFailed(e.to_string()))?;

    let mut failures = Vec::new();
    let mut ok_count = 0usize;
    for target_id in targets {
        let body = serde_json::json!({
            id_key: target_id,
            "message": text,
        });
        let mut req = client.post(&url).json(&body);
        if !access_token.is_empty() {
            req = req.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            );
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => ok_count += 1,
            Ok(resp) => failures.push(format!("{target_id}: HTTP {}", resp.status().as_u16())),
            Err(err) => failures.push(format!("{target_id}: {err}")),
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    if ok_count == 0 {
        return Err(SendError::AllFailed(failures.join("; ")));
    }
    Err(SendError::Partial(format!(
        "OneBot 部分失败({ok_count} 成功): {}",
        failures.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{BotId, OfflineAlertKind, OfflineAlertSource};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FakeResolver(Option<u16>);
    impl PortResolver for FakeResolver {
        fn published_port(&self, _container: &str, _port: u16) -> Option<u16> {
            self.0
        }
    }

    fn settings(messengers: Vec<WatchOneBotMessenger>) -> WatchOneBotSettings {
        WatchOneBotSettings {
            enabled: true,
            messengers,
            target_type: "private".into(),
            target_ids: vec![1],
            message_template: "hi {uin}".into(),
        }
    }

    fn m(id: &str, url: &str) -> WatchOneBotMessenger {
        WatchOneBotMessenger {
            bot_id: id.into(),
            base_url: url.into(),
            access_token: String::new(),
            container_name: None,
            container_port: None,
        }
    }

    fn docker_m(id: &str, url: &str, port: u16) -> WatchOneBotMessenger {
        WatchOneBotMessenger {
            container_name: Some(format!("ncbot-{id}")),
            container_port: Some(port),
            ..m(id, url)
        }
    }

    fn alert() -> OfflineAlert {
        OfflineAlert {
            bot_id: BotId::new("10001"),
            qq_id: 10001,
            bot_name: "a".into(),
            kind: OfflineAlertKind::Manual,
            source: OfflineAlertSource::NapCat,
            at: "now".into(),
        }
    }

    #[test]
    fn candidates_skip_offline_bot_and_keep_order() {
        let s = settings(vec![
            m("10001", "http://127.0.0.1:3001"),
            m("10002", "http://127.0.0.1:3002"),
            m("10003", "http://127.0.0.1:3003"),
        ]);
        let ids: Vec<_> = candidate_messengers(&s, "10001", None)
            .iter()
            .map(|m| m.bot_id.as_str())
            .collect();
        assert_eq!(ids, ["10002", "10003"]);
    }

    #[test]
    fn candidates_respect_online_set_and_disabled() {
        let mut s = settings(vec![m("10002", "http://127.0.0.1:3002")]);
        let mut online = HashSet::new();
        online.insert("10003".to_string());
        assert!(candidate_messengers(&s, "10001", Some(&online)).is_empty());
        online.insert("10002".to_string());
        assert_eq!(candidate_messengers(&s, "10001", Some(&online)).len(), 1);
        s.enabled = false;
        assert!(candidate_messengers(&s, "10001", None).is_empty());
    }

    #[test]
    fn resolve_prefers_actual_docker_mapping() {
        let dm = docker_m("10002", "http://127.0.0.1:3216", 3000);
        assert_eq!(
            resolve_base_url(&dm, &FakeResolver(Some(3999))).as_deref(),
            Some("http://127.0.0.1:3999")
        );
        // 查不到映射时回退桌面端按规则推的地址
        assert_eq!(
            resolve_base_url(&dm, &FakeResolver(None)).as_deref(),
            Some("http://127.0.0.1:3216")
        );
        // 规则也推不出、容器里也没映射:没有可连的地址
        let unpublished = docker_m("10002", "", 3010);
        assert_eq!(resolve_base_url(&unpublished, &FakeResolver(None)), None);
        assert_eq!(
            resolve_base_url(&m("10002", "  "), &FakeResolver(Some(1))),
            None
        );
    }

    #[tokio::test]
    async fn falls_back_to_next_messenger_when_first_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send_private_msg"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let s = settings(vec![
            // 没有地址的直接跳过,连不上的换下一个
            docker_m("10002", "", 3010),
            m("10003", "http://127.0.0.1:1"),
            m("10004", &server.uri()),
        ]);
        send_watch_onebot_with(&s, "10001", None, &alert(), &FakeResolver(None))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reports_every_messenger_when_all_fail() {
        let s = settings(vec![
            docker_m("10002", "", 3010),
            m("10003", "http://127.0.0.1:1"),
        ]);
        let err = send_watch_onebot_with(&s, "10001", None, &alert(), &FakeResolver(None))
            .await
            .unwrap_err();
        assert!(err.contains("10002") && err.contains("10003"), "{err}");

        let only_self = settings(vec![m("10001", "http://127.0.0.1:1")]);
        let err = send_watch_onebot_with(&only_self, "10001", None, &alert(), &FakeResolver(None))
            .await
            .unwrap_err();
        assert!(err.contains("无可用"), "{err}");
    }

    #[tokio::test]
    async fn partial_failure_does_not_resend_via_next_messenger() {
        let first = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(|req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                if body["user_id"] == 1 {
                    ResponseTemplate::new(200)
                } else {
                    ResponseTemplate::new(500)
                }
            })
            .mount(&first)
            .await;
        let second = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&second)
            .await;
        let mut s = settings(vec![m("10002", &first.uri()), m("10003", &second.uri())]);
        s.target_ids = vec![1, 2];
        let err = send_watch_onebot_with(&s, "10001", None, &alert(), &FakeResolver(None))
            .await
            .unwrap_err();
        assert!(err.contains("部分失败"), "{err}");
    }
}
