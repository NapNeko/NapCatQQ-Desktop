//! SCM 承载同一值守引擎；所有原生账号事实和取码均回到所属会话。
use async_trait::async_trait;
use ncd_domain::{BotActorState, SnowLumaQrLoginSnapshot, windows_remote::*};
use ncd_relogin::{AccountState, Observation};
use ncd_watch::{LoginStatus, NotifyBotTarget, ProbeKind, ProbeResult, ProbeStatus, WatchPaths};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

struct Adapter {
    root: PathBuf,
    webui: Mutex<ncd_watch::relogin::webui::WebUi>,
}

impl Adapter {
    fn call_sync(&self, action: WindowsAgentAction) -> Result<WindowsAgentResult, String> {
        let request = WindowsAgentRequest {
            v: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            action,
        };
        let response = crate::ipc::request(&self.root, None, &request)?;
        match response.result {
            WindowsAgentResult::Error(error) => Err(format!("{}: {}", error.code, error.message)),
            result => Ok(result),
        }
    }
    async fn call(&self, action: WindowsAgentAction) -> Result<WindowsAgentResult, String> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let request = WindowsAgentRequest {
                v: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
                action,
            };
            let result = crate::ipc::request(&root, None, &request)?.result;
            if let WindowsAgentResult::Error(error) = result {
                return Err(format!("{}: {}", error.code, error.message));
            }
            Ok(result)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[async_trait]
impl ncd_watch::platform::NativeWatchAdapter for Adapter {
    fn process(&self, bot: &NotifyBotTarget) -> ProbeResult {
        let status = match self.call_sync(WindowsAgentAction::BotStatus {
            bot_id: bot.bot_id.clone(),
        }) {
            Ok(WindowsAgentResult::BotStatus(status)) if status.state.is_active() => {
                ProbeStatus::Online
            }
            Ok(WindowsAgentResult::BotStatus(status))
                if matches!(
                    status.state,
                    BotActorState::Stopped | BotActorState::Crashed
                ) =>
            {
                ProbeStatus::Offline
            }
            _ => ProbeStatus::Unknown,
        };
        ProbeResult {
            bot_id: bot.bot_id.clone(),
            kind: ProbeKind::PidFile,
            status,
            login: LoginStatus::Unknown,
            detail: "Windows user-session supervisor".into(),
        }
    }

    async fn observe(&self, bot: &NotifyBotTarget) -> Option<Observation> {
        let mut observation = Observation {
            bot_id: bot.bot_id.clone(),
            name: bot.bot_name.clone(),
            generation: String::new(),
            state: AccountState::Unknown,
            reset_generation: String::new(),
        };
        let Ok(WindowsAgentResult::BotStatus(status)) = self
            .call(WindowsAgentAction::BotStatus {
                bot_id: bot.bot_id.clone(),
            })
            .await
        else {
            return Some(observation);
        };
        if !status.state.is_active() {
            observation.state = AccountState::Offline;
            observation.generation = format!("{}:stopped", bot.bot_id);
            return Some(observation);
        }
        observation.generation = status
            .extra
            .get("agent_generation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if observation.generation.is_empty() {
            return Some(observation);
        }
        let Ok(WindowsAgentResult::BotEndpoint(endpoint)) = self
            .call(WindowsAgentAction::BotEndpoint {
                bot_id: bot.bot_id.clone(),
            })
            .await
        else {
            return Some(observation);
        };
        let sl = bot.backend.eq_ignore_ascii_case("snowluma");
        let mut webui = self.webui.lock().await;
        if sl {
            let data = webui
                .request(
                    &bot.bot_id,
                    endpoint.host_port,
                    &endpoint.secret,
                    true,
                    "/api/processes",
                )
                .await
                .ok()?;
            let processes: ncd_watch::relogin::webui::Processes =
                serde_json::from_value(data).ok()?;
            let candidates: Vec<_> = processes
                .list
                .iter()
                .filter(|process| Some(process.pid) == status.pid)
                .collect();
            if candidates.len() == 1 {
                observation.state =
                    ncd_watch::relogin::webui::account_state(candidates[0], &bot.bot_id);
            }
        } else {
            let state = webui
                .request(
                    &bot.bot_id,
                    endpoint.host_port,
                    &endpoint.secret,
                    false,
                    "/api/QQLogin/CheckLoginStatus",
                )
                .await
                .ok()?;
            let info = webui
                .request(
                    &bot.bot_id,
                    endpoint.host_port,
                    &endpoint.secret,
                    false,
                    "/api/QQLogin/GetQQLoginInfo",
                )
                .await
                .ok()?;
            let account = info
                .get("uin")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !account.is_empty() && account != bot.bot_id {
                return Some(observation);
            }
            observation.state = match info.get("online").and_then(serde_json::Value::as_bool) {
                Some(true) if account == bot.bot_id => AccountState::Online,
                Some(false) => AccountState::Offline,
                None if state.get("isLogin").and_then(serde_json::Value::as_bool)
                    == Some(false)
                    || state.get("isOffline").and_then(serde_json::Value::as_bool)
                        == Some(true) =>
                {
                    AccountState::Offline
                }
                _ => AccountState::Unknown,
            };
        }
        Some(observation)
    }

    async fn qr(&self, bot_id: &str, refresh: bool) -> Result<SnowLumaQrLoginSnapshot, String> {
        match self
            .call(WindowsAgentAction::BotQr {
                bot_id: bot_id.into(),
                refresh,
            })
            .await?
        {
            WindowsAgentResult::QrLogin(snapshot) => Ok(snapshot),
            _ => Err("unexpected QR helper response".into()),
        }
    }
    async fn close_qr(&self, bot_id: &str) {
        let _ = self
            .call(WindowsAgentAction::BotCloseQr {
                bot_id: bot_id.into(),
            })
            .await;
    }
    async fn read_webui(&self, bot_id: &str, path: &str) -> Result<serde_json::Value, String> {
        let WindowsAgentResult::BotEndpoint(endpoint) = self
            .call(WindowsAgentAction::BotEndpoint {
                bot_id: bot_id.into(),
            })
            .await?
        else {
            return Err("Bot WebUI endpoint is not ready".into());
        };
        let snowluma = endpoint.backend == ncd_domain::BackendType::SnowLuma;
        self.webui
            .lock()
            .await
            .request(bot_id, endpoint.host_port, &endpoint.secret, snowluma, path)
            .await
    }
}

pub fn initialize(root: &std::path::Path, owner: &str) -> Result<WatchPaths, String> {
    let user_root = root.join("users").join(crate::store::key(owner));
    let paths = WatchPaths::from_root(&user_root.join("ncd-watch"));
    for directory in [&paths.config_dir, &paths.state_dir, &paths.log_dir] {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    }
    if !paths.watch_json.exists() {
        crate::store::write(&paths.watch_json, &ncd_watch::WatchConfig::default())?;
    }
    let adapter = Adapter {
        root: root.to_path_buf(),
        webui: Mutex::new(ncd_watch::relogin::webui::WebUi::new()?),
    };
    ncd_watch::platform::install(Arc::new(adapter))?;
    Ok(paths)
}

pub async fn run(paths: WatchPaths, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    let disabled = paths.config_dir.join("disabled");
    loop {
        if *shutdown.borrow() {
            return;
        }
        if disabled.exists() {
            tokio::select! { _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}, _ = shutdown.changed() => return }
            continue;
        }
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let worker = tokio::spawn(ncd_watch::run_loop(
            paths.clone(),
            Arc::new(ncd_watch::probe::HostProber),
            receiver,
        ));
        loop {
            tokio::select! { _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}, _ = shutdown.changed() => {} }
            if disabled.exists() || *shutdown.borrow() {
                break;
            }
        }
        let _ = cancel.send(true);
        let _ = worker.await;
    }
}
