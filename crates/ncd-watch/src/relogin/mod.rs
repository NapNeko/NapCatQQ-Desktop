// watch 常驻重登闭环，不依赖 Desktop 心跳。
mod capture;
#[cfg(test)]
mod tests;
mod webui;

use crate::config::{NotifyConfig, WatchPaths};
use crate::relogin_config::{WatchEventEndpoint, WatchLoginTarget};
use async_trait::async_trait;
use base64::Engine;
use ncd_domain::{SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};
use ncd_onebot::client::{ClientError, WsClient, connect_ws};
use ncd_relogin::{
    AccountState, Delivery, Observation, Outgoing, QrReply, ReloginManager, ReloginPort,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, mpsc};

pub struct WatchPort {
    config_path: std::path::PathBuf,
    config: NotifyConfig,
    webui: Mutex<webui::WebUi>,
    connections: HashMap<String, WsClient>,
    processes: Mutex<HashMap<String, webui::Process>>,
}

impl WatchPort {
    fn target(&self, bot: &str) -> Option<&WatchLoginTarget> {
        self.config
            .onebot
            .relogin
            .login_targets
            .iter()
            .find(|t| t.bot_id == bot)
    }
    async fn port(&self, target: &WatchLoginTarget) -> Option<u16> {
        match (&target.container_name, target.container_webui_port) {
            (Some(container), Some(port)) => capture::docker_port(container, port).await,
            _ => (target.webui_port > 0).then_some(target.webui_port),
        }
    }
    async fn read(&self, bot: &str, path: &str) -> Result<Value, String> {
        let target = self.target(bot).ok_or("缺少 watch 登录配置")?;
        let port = self.port(target).await.ok_or("WebUI 端口不可用")?;
        let sl = self
            .config
            .bots
            .iter()
            .any(|b| b.bot_id == bot && b.backend == "snowluma");
        self.webui
            .lock()
            .await
            .request(bot, port, &target.webui_secret, sl, path)
            .await
    }
    async fn sl_process(&self, bot: &str) -> Option<webui::Process> {
        let target = self.target(bot)?;
        let data = self.read(bot, "/api/processes").await.ok()?;
        let processes: webui::Processes = serde_json::from_value(data).ok()?;
        let pid = if let Some(file) = &target.pid_file {
            let bytes = capture::read_limited(file, 32).await.ok()?;
            std::str::from_utf8(&bytes).ok()?.trim().parse::<u32>().ok()
        } else if let Some(container) = &target.container_name {
            capture::docker_anchor_pid(container)
                .await
                .or(self.processes.lock().await.get(bot).map(|p| p.pid))
                .or(target.expected_pid)
        } else {
            self.processes
                .lock()
                .await
                .get(bot)
                .map(|p| p.pid)
                .or(target.expected_pid)
        };
        let by_account: Vec<_> = processes.list.iter().filter(|p| p.uin == bot).collect();
        let process = if by_account.len() == 1 {
            by_account[0].clone()
        } else if by_account.is_empty() {
            processes.list.into_iter().find(|p| Some(p.pid) == pid)?
        } else {
            return None;
        };
        if process.pid == 0 || (!process.uin.is_empty() && process.uin != bot) {
            return None;
        }
        self.processes
            .lock()
            .await
            .insert(bot.to_owned(), process.clone());
        Some(process)
    }
}

#[async_trait]
impl ReloginPort for WatchPort {
    async fn observe(&self, bot: &str) -> Option<Observation> {
        let config = self.config.bots.iter().find(|b| b.bot_id == bot)?;
        let mut result = Observation {
            bot_id: bot.to_owned(),
            name: config.bot_name.clone(),
            generation: String::new(),
            state: AccountState::Unknown,
            reset_generation: String::new(),
        };
        if !config.enabled {
            result.state = AccountState::Stopped;
            return Some(result);
        }
        let intent = self
            .config_path
            .parent()?
            .parent()?
            .join("state")
            .join("intents")
            .join(format!("{}.json", config.qq_id));
        if intent.exists() {
            match capture::read_limited(intent.to_str()?, 1024)
                .await
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<crate::relogin_config::WatchRunIntent>(&bytes).ok()
                }) {
                Some(intent) => {
                    result.reset_generation = intent.reset_generation;
                    if !intent.running {
                        result.state = AccountState::Stopped;
                        return Some(result);
                    }
                }
                None => return Some(result),
            }
        }
        if capture::process_absent(config, self.target(bot)).await == Some(true) {
            result.state = AccountState::Offline;
            result.generation = format!("{}:process-down", result.reset_generation);
            return Some(result);
        }
        if config.backend == "snowluma" {
            if let Some(process) = self.sl_process(bot).await {
                result.state = webui::account_state(&process, bot);
                if let Some(target) = self.target(bot) {
                    result.generation =
                        capture::generation(target.container_name.as_deref(), process.pid)
                            .await
                            .unwrap_or_default();
                    if result.generation.is_empty() {
                        result.state = AccountState::Unknown;
                    }
                }
            }
        } else if let Ok(status) = self.read(bot, "/api/QQLogin/CheckLoginStatus").await {
            let info = self.read(bot, "/api/QQLogin/GetQQLoginInfo").await.ok();
            let online = info
                .as_ref()
                .and_then(|v| v.get("online"))
                .and_then(Value::as_bool);
            let uin = info
                .as_ref()
                .and_then(|v| v.get("uin"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !uin.is_empty() && uin != bot {
                return Some(result);
            }
            result.state = match online {
                Some(true) if uin == bot => AccountState::Online,
                Some(false) => AccountState::Offline,
                None if status.get("isOffline").and_then(Value::as_bool) == Some(true)
                    || status.get("isLogin").and_then(Value::as_bool) == Some(false) =>
                {
                    AccountState::Offline
                }
                _ => AccountState::Unknown,
            };
            result.generation = format!("nc:{}", self.target(bot)?.webui_port);
        }
        // 传输不可用保持独立，不覆盖 WebUI 给出的登录态。
        if result.state == AccountState::Unknown {
            if let Some(client) = self.connections.get(bot).filter(|c| !c.is_closed()) {
                let login = client
                    .call("get_login_info", &json!({}), Duration::from_secs(5))
                    .await
                    .ok();
                let matches = login
                    .as_ref()
                    .and_then(|r| r.value.get("data"))
                    .and_then(|d| d.get("user_id"))
                    .and_then(Value::as_u64)
                    .map(|u| u.to_string())
                    .as_deref()
                    == Some(bot);
                if matches && result.state == AccountState::Unknown {
                    let status = client
                        .call("get_status", &json!({}), Duration::from_secs(5))
                        .await
                        .ok();
                    if status
                        .as_ref()
                        .and_then(|r| r.value.get("data"))
                        .and_then(|d| d.get("online"))
                        == Some(&Value::Bool(true))
                    {
                        result.state = AccountState::Online;
                    }
                }
            }
        }
        if !result.reset_generation.is_empty() {
            result.generation = format!("{}:{}", result.reset_generation, result.generation);
        }
        Some(result)
    }

    async fn can_send(&self, bot: &str) -> bool {
        let Some(client) = self.connections.get(bot).filter(|c| !c.is_closed()) else {
            return false;
        };
        let login = client
            .call("get_login_info", &json!({}), Duration::from_secs(3))
            .await
            .ok();
        let matches = login
            .as_ref()
            .filter(|r| successful(&r.value))
            .and_then(|r| r.value.get("data"))
            .and_then(|v| v.get("user_id"))
            .and_then(Value::as_u64)
            .is_some_and(|id| id.to_string() == bot);
        if !matches {
            return false;
        }
        client
            .call("get_status", &json!({}), Duration::from_secs(3))
            .await
            .ok()
            .filter(|r| successful(&r.value))
            .and_then(|r| r.value.get("data").cloned())
            .and_then(|v| v.get("online").and_then(Value::as_bool))
            == Some(true)
    }

    async fn authorized(&self, bot: &str, administrator: Option<u64>) -> bool {
        let Ok(current) = NotifyConfig::load_or_default(&self.config_path) else {
            return false;
        };
        let settings = &current.onebot.relogin.settings;
        current.onebot.enabled
            && settings.enabled
            && !settings.administrator_ids.is_empty()
            && (bot.is_empty()
                || (settings.monitors(bot)
                    && current.bots.iter().any(|b| b.bot_id == bot && b.enabled)))
            && administrator.is_none_or(|id| settings.administrator_ids.contains(&id))
            && current.onebot.relogin == self.config.onebot.relogin
            && current.onebot.target_type == self.config.onebot.target_type
            && current.onebot.target_ids == self.config.onebot.target_ids
    }

    async fn qr(&self, bot: &str, refresh: bool) -> Result<QrReply, String> {
        if !self.authorized(bot, None).await {
            return Err("配置已改变".into());
        }
        let before = self.observe(bot).await.ok_or("账号不存在")?;
        if before.state != AccountState::Offline {
            return Err("账号并非确定离线".into());
        }
        let target = self.target(bot).ok_or("缺少登录配置")?;
        if !target.qr_enabled {
            return Err("扫码辅助未启用".into());
        }
        let config = self
            .config
            .bots
            .iter()
            .find(|b| b.bot_id == bot)
            .ok_or("账号不存在")?;
        let snapshot = if config.backend == "snowluma" {
            let process = self.sl_process(bot).await.ok_or("QQ 进程不可用")?;
            if webui::account_state(&process, bot) != AccountState::Offline {
                return Err("登录状态已改变".into());
            }
            let binding =
                capture::binding(target.container_name.as_deref(), process.pid, &process.path)
                    .await?;
            let result =
                capture::capture(target.container_name.as_deref(), &binding, refresh).await?;
            // PID 复用或容器重建后不能发出旧图。
            let current =
                capture::binding(target.container_name.as_deref(), process.pid, &process.path)
                    .await?;
            if current != binding {
                return Err("QQ 进程代次已改变".into());
            }
            result
        } else {
            let status = self.read(bot, "/api/QQLogin/CheckLoginStatus").await?;
            let phase = status.get("loginPhase").and_then(Value::as_str);
            if status.get("qrLoginAccepted").and_then(Value::as_bool) == Some(true)
                || matches!(phase, Some("qrcode_scanned" | "initializing"))
            {
                return Ok(QrReply {
                    generation: before.generation,
                    snapshot: SnowLumaQrLoginSnapshot {
                        v: 1,
                        state: SnowLumaQrLoginState::Scanned,
                        qq_pid: None,
                        qrcode_payload: None,
                        uin: None,
                        observer_removed: true,
                        message: None,
                    },
                });
            }
            if status.get("isLogin").and_then(Value::as_bool) == Some(true)
                || matches!(phase, Some("reconnecting" | "generating_qrcode"))
            {
                return Err("正在恢复登录，请稍后再试".into());
            }
            let status = if refresh {
                // 旧 NC 不提供扫码阶段时只能重读当前码，避免打断手机确认。
                if phase == Some("waiting_qrcode") || phase == Some("offline") {
                    let _ = self.read(bot, "/api/QQLogin/RefreshQRcode").await?;
                    self.read(bot, "/api/QQLogin/CheckLoginStatus").await?
                } else {
                    status
                }
            } else {
                status
            };
            if status.get("qrLoginAccepted").and_then(Value::as_bool) == Some(true)
                || status.get("isLogin").and_then(Value::as_bool) == Some(true)
                || matches!(
                    status.get("loginPhase").and_then(Value::as_str),
                    Some("qrcode_scanned" | "initializing" | "reconnecting" | "generating_qrcode")
                )
            {
                return Err("登录阶段已改变".into());
            }
            let payload = status
                .get("qrcodeurl")
                .and_then(Value::as_str)
                .ok_or("尚无二维码")?;
            let image = qr_image(payload)?;
            SnowLumaQrLoginSnapshot {
                v: 1,
                state: SnowLumaQrLoginState::WaitingForQr,
                qq_pid: None,
                qrcode_payload: Some(image),
                uin: None,
                observer_removed: true,
                message: None,
            }
        };
        Ok(QrReply {
            generation: before.generation,
            snapshot,
        })
    }

    async fn send(&self, messenger: &str, recipient: u64, message: Outgoing) -> Delivery {
        if !self.authorized("", Some(recipient)).await {
            return Delivery::Rejected;
        }
        let Some(client) = self.connections.get(messenger) else {
            return Delivery::Rejected;
        };
        let mut segments = vec![json!({"type":"text","data":{"text":message.text}})];
        if let Some(image) = message.image {
            segments.push(json!({"type":"image","data":{"file":image}}));
        }
        delivery(
            client
                .call(
                    "send_private_msg",
                    &json!({"user_id":recipient,"message":segments}),
                    Duration::from_secs(10),
                )
                .await,
        )
    }

    async fn qr_is_current(&self, bot: &str, qr: &QrReply) -> bool {
        // 再读当前控件；已扫码、Desktop 刷新过或登录恢复时丢弃旧图。
        self.qr(bot, false)
            .await
            .is_ok_and(|current| current == *qr)
    }

    async fn send_group(&self, messenger: &str, group: u64, text: String) -> Delivery {
        if !self.authorized("", None).await
            || self.config.onebot.target_type != "group"
            || !self.config.onebot.target_ids.contains(&group)
        {
            return Delivery::Rejected;
        }
        let Some(client) = self.connections.get(messenger) else {
            return Delivery::Rejected;
        };
        delivery(
            client
                .call(
                    "send_group_msg",
                    &json!({"group_id":group,"message":[{"type":"text","data":{"text":text}}]}),
                    Duration::from_secs(10),
                )
                .await,
        )
    }

    async fn fallback(&self, bot: &str) {
        let Some(target) = self.config.bots.iter().find(|b| b.bot_id == bot) else {
            return;
        };
        let alert =
            crate::webhook::build_offline_alert(target, ncd_domain::OfflineAlertKind::Manual);
        if self.config.webhook_enabled {
            if crate::webhook::send_watch_webhooks(&self.config.enabled_webhooks(), &alert)
                .await
                .is_err()
            {
                tracing::warn!(bot_id = bot, "relogin fallback webhook failed");
            }
        }
        if self.config.email_enabled {
            let email = self.config.email.clone();
            let _ =
                tokio::task::spawn_blocking(move || crate::email::send_watch_email(&email, &alert))
                    .await;
        }
    }
    async fn close_qr(&self, _bot: &str) {}
}

fn successful(value: &Value) -> bool {
    value.get("status").and_then(Value::as_str) == Some("ok")
        && value.get("retcode").and_then(Value::as_i64) == Some(0)
}

fn delivery(result: Result<ncd_onebot::client::RawCall, ClientError>) -> Delivery {
    match result {
        Ok(raw)
            if raw.value.get("status").and_then(Value::as_str) == Some("ok")
                && raw.value.get("retcode").and_then(Value::as_i64) == Some(0) =>
        {
            let id = raw.value.get("data").and_then(|d| d.get("message_id"));
            let id = id.and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            });
            id.map(|message_id| Delivery::Sent { message_id })
                .unwrap_or(Delivery::Unknown)
        }
        Ok(raw) if raw.value.get("status").and_then(Value::as_str) == Some("failed") => {
            Delivery::Rejected
        }
        Err(ClientError::NotSent | ClientError::Connect(_) | ClientError::Unauthorized(_)) => {
            Delivery::Rejected
        }
        _ => Delivery::Unknown,
    }
}

fn qr_image(payload: &str) -> Result<String, String> {
    if payload.starts_with("data:image/png;base64,") && payload.len() <= 512 * 1024 {
        return Ok(payload.to_owned());
    }
    if payload.len() > 4096 {
        return Err("二维码内容过大".into());
    }
    let url = reqwest::Url::parse(payload).map_err(|_| "二维码地址无效")?;
    if url.scheme() != "https"
        || url.host_str() != Some("txz.qq.com")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("二维码地址不受支持".into());
    }
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|_| "二维码编码失败")?;
    let image = code
        .render::<image::Luma<u8>>()
        .min_dimensions(320, 320)
        .build();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "二维码图片编码失败")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

pub async fn run(paths: WatchPaths, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    let mut manager = match ReloginManager::load(paths.state_dir.join("relogin.json")) {
        Ok(manager) => manager,
        Err(error) => {
            tracing::error!(%error, "relogin unavailable");
            return;
        }
    };
    let webui = match webui::WebUi::new() {
        Ok(webui) => webui,
        Err(_) => return,
    };
    let mut port = WatchPort {
        config_path: paths.notify_json.clone(),
        config: NotifyConfig::default(),
        webui: Mutex::new(webui),
        connections: HashMap::new(),
        processes: Mutex::new(HashMap::new()),
    };
    let mut endpoints: HashMap<String, WatchEventEndpoint> = HashMap::new();
    let mut events: HashMap<String, mpsc::Receiver<Value>> = HashMap::new();
    let mut retries: HashMap<String, Instant> = HashMap::new();
    let mut last_probe = Instant::now()
        .checked_sub(Duration::from_secs(15))
        .unwrap_or_else(Instant::now);
    let mut last_status = Instant::now()
        .checked_sub(Duration::from_secs(10))
        .unwrap_or_else(Instant::now);
    loop {
        if *shutdown.borrow() {
            break;
        }
        let mut config = match NotifyConfig::load_or_default(&paths.notify_json) {
            Ok(config) => config,
            Err(_) => {
                for client in port.connections.values() {
                    client.close();
                }
                port.connections.clear();
                events.clear();
                tokio::select! { _ = tokio::time::sleep(Duration::from_secs(2)) => {}, _ = shutdown.changed() => break }
                continue;
            }
        };
        config.onebot.relogin.settings.normalize();
        let settings = if config.onebot.enabled && !config.onebot.relogin.event_endpoints.is_empty()
        {
            config.onebot.relogin.settings.clone()
        } else {
            Default::default()
        };
        manager.configure(
            settings.clone(),
            config
                .onebot
                .relogin
                .event_endpoints
                .iter()
                .map(|m| m.bot_id.clone())
                .collect(),
        );
        manager.delivery_options(
            if config.onebot.target_type == "group" {
                config.onebot.target_ids.clone()
            } else {
                vec![]
            },
            config.notify_on_recovered,
        );
        let active: HashMap<_, _> = config
            .onebot
            .relogin
            .event_endpoints
            .iter()
            .filter(|_| settings.enabled)
            .take(64)
            .map(|e| (e.bot_id.clone(), e.clone()))
            .collect();
        port.connections.retain(|id, client| {
            let keep = active.get(id) == endpoints.get(id)
                && active.contains_key(id)
                && !client.is_closed();
            if !keep {
                client.close();
            }
            keep
        });
        events.retain(|id, _| port.connections.contains_key(id));
        retries.retain(|id, _| active.contains_key(id));
        port.config = config;
        let ids: Vec<_> = port.config.bots.iter().map(|b| b.bot_id.clone()).collect();
        port.webui.lock().await.prune();
        port.processes.lock().await.retain(|id, _| ids.contains(id));
        for (id, endpoint) in &active {
            if port.connections.contains_key(id)
                || retries
                    .get(id)
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(30))
            {
                continue;
            }
            retries.insert(id.clone(), Instant::now());
            let local_port = match &endpoint.container_name {
                Some(container) => capture::docker_port(container, endpoint.port).await,
                None => Some(endpoint.port),
            };
            let Some(local_port) = local_port.filter(|p| *p > 0) else {
                continue;
            };
            if !endpoint.path.starts_with('/') || endpoint.path.contains(['?', '#']) {
                continue;
            }
            let url = format!("ws://127.0.0.1:{local_port}{}", endpoint.path);
            let (tx, rx) = mpsc::channel(128);
            match tokio::time::timeout(
                Duration::from_secs(5),
                connect_ws(&url, Some(&endpoint.token), tx),
            )
            .await
            {
                Ok(Ok(client)) => {
                    port.connections.insert(id.clone(), client);
                    events.insert(id.clone(), rx);
                }
                _ => tracing::debug!(bot_id = %id, "relogin receiver unavailable"),
            }
        }
        endpoints = active;
        let now = chrono::Utc::now().timestamp();
        let status = crate::relogin_config::WatchReloginStatus {
            v: 1,
            updated_at: now,
            configuration: crate::relogin_config::fingerprint(&port.config).unwrap_or_default(),
            connected_messengers: port.connections.keys().cloned().collect(),
        };
        if last_status.elapsed() >= Duration::from_secs(10) {
            if let Ok(bytes) = serde_json::to_vec(&status) {
                let _ =
                    ncd_relogin::atomic_write(&paths.state_dir.join("relogin-status.json"), &bytes);
            }
            last_status = Instant::now();
        }
        if last_probe.elapsed() >= Duration::from_secs(15) {
            last_probe = Instant::now();
            let mut observations = Vec::new();
            for bot in port
                .config
                .bots
                .iter()
                .filter(|_| settings.enabled)
                .take(256)
            {
                let observation =
                    tokio::time::timeout(Duration::from_secs(12), port.observe(&bot.bot_id))
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| Observation {
                            bot_id: bot.bot_id.clone(),
                            name: bot.bot_name.clone(),
                            generation: String::new(),
                            state: AccountState::Unknown,
                            reset_generation: String::new(),
                        });
                observations.push(observation);
            }
            if let Err(error) = manager.tick(&observations, now, &port).await {
                tracing::error!(%error, "relogin tick failed");
                break;
            }
        }
        let groups = if port.config.onebot.target_type == "group" {
            port.config.onebot.target_ids.clone()
        } else {
            vec![]
        };
        for (id, receiver) in &mut events {
            for _ in 0..32 {
                let Ok(event) = receiver.try_recv() else {
                    break;
                };
                if let Some(command) =
                    ncd_relogin::parse_event(id, &event, &groups, chrono::Utc::now().timestamp())
                {
                    if let Err(error) = manager
                        .command(command, chrono::Utc::now().timestamp(), &port)
                        .await
                    {
                        tracing::error!(%error, "relogin command failed");
                    }
                }
            }
        }
        tokio::select! { _ = tokio::time::sleep(Duration::from_secs(1)) => {}, _ = shutdown.changed() => break }
    }
    for client in port.connections.values() {
        client.close();
    }
}
