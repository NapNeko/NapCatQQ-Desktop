//! ServerManager：远端主机档案管理 + 凭据存储 + 连接测试。
//!
//! 这个模块负责：
//! 1. ServerProfile CRUD（JSON 持久化到 `<data_root>/config/servers.json`）
//! 2. 凭据（密码 / 私钥密码）走 keyring 系统凭据库，不落盘
//! 3. 连接测试（SSH 握手 + 基本信息探测）
//! 4. 活跃 Host 连接缓存
//!
//! 不负责部署编排（那是 Deployment trait 的事）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;

use ncd_host::remote::{ConnectionConfig, HostKeyPolicy, RemoteLinuxHost, SshCredentials};
use ncd_host::Host;

use ncd_component::{Component, ComponentId};

use crate::events::EventBus;

// ============================================================
// 数据结构
// ============================================================

/// 远端主机档案。不含密码——凭据走 keyring。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ServerProfile {
    /// 内部 id，创建时生成的短 UUID。
    pub id: String,
    /// 用户给的显示名称。
    pub name: String,
    /// 主机地址（IP 或域名）。
    pub host: String,
    /// SSH 端口，默认 22。
    #[serde(default = "default_port")]
    pub port: u16,
    /// 登录用户名。
    pub username: String,
    /// 认证方式。
    #[serde(default)]
    pub auth_method: AuthMethod,
    /// 私钥文件路径（仅 Key 方式使用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// 用户是否选择了"记住密码"。
    #[serde(default)]
    pub remember_credential: bool,
    /// 最近一次连接测试结果。
    #[serde(default)]
    pub state: ServerState,
    /// WebUI 端点 URL（用户手填的远端 NapCat WebUI 地址）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_url: Option<String>,
}

/// 认证方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AuthMethod {
    /// 私钥认证（推荐）。
    #[default]
    Key,
    /// 密码认证。
    Password,
}

/// 主机连接状态。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ServerState {
    /// 未连接 / 未测试过。
    #[default]
    Disconnected,
    /// 连接中。
    Connecting,
    /// 连接成功。
    Connected,
    /// 连接失败。
    Failed,
}

/// test_connection 返回的探测报告。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ProbeReport {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_info: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub latency_ms: u64,
}

fn default_port() -> u16 {
    22
}

// ============================================================
// 凭据存储
// ============================================================

const KEYRING_SERVICE: &str = "napcatqq-desktop";

/// 系统凭据库操作。测试时可 mock。
pub trait ServerCredentialStore: Send + Sync {
    fn get_password(&self, server_id: &str) -> Option<String>;
    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String>;
    fn delete_password(&self, server_id: &str) -> Result<(), String>;
}

/// 基于 keyring crate 的生产实装（Windows wincred / macOS Keychain / Linux secret-service）。
pub struct KeyringCredentialStore;

impl ServerCredentialStore for KeyringCredentialStore {
    fn get_password(&self, server_id: &str) -> Option<String> {
        let account = format!("ssh:{server_id}");
        keyring::Entry::new(KEYRING_SERVICE, &account)
            .ok()
            .and_then(|entry| entry.get_password().ok())
    }

    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        let account = format!("ssh:{server_id}");
        keyring::Entry::new(KEYRING_SERVICE, &account)
            .map_err(|e| e.to_string())?
            .set_password(password)
            .map_err(|e| e.to_string())
    }

    fn delete_password(&self, server_id: &str) -> Result<(), String> {
        let account = format!("ssh:{server_id}");
        match keyring::Entry::new(KEYRING_SERVICE, &account) {
            Ok(entry) => {
                let _ = entry.delete_password();
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }
}

/// 内存 mock（测试用）。
#[derive(Default)]
pub struct InMemoryCredentialStore {
    store: std::sync::Mutex<HashMap<String, String>>,
}

impl ServerCredentialStore for InMemoryCredentialStore {
    fn get_password(&self, server_id: &str) -> Option<String> {
        self.store.lock().unwrap().get(server_id).cloned()
    }

    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .insert(server_id.to_string(), password.to_string());
        Ok(())
    }

    fn delete_password(&self, server_id: &str) -> Result<(), String> {
        self.store.lock().unwrap().remove(server_id);
        Ok(())
    }
}

// ============================================================
// ServerProfileRepo：JSON 持久化
// ============================================================

/// servers.json 路径固定在 `<data_root>/config/servers.json`。
struct ServerProfileRepo {
    path: PathBuf,
}

impl ServerProfileRepo {
    fn new(data_root: &Path) -> Self {
        Self {
            path: data_root.join("config").join("servers.json"),
        }
    }

    async fn load(&self) -> Vec<ServerProfile> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    async fn save(&self, profiles: &[ServerProfile]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        let json =
            serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
        tokio::fs::write(&self.path, json)
            .await
            .map_err(|e| e.to_string())
    }
}

// ============================================================
// ServerManager
// ============================================================

pub struct ServerManager {
    repo: ServerProfileRepo,
    credentials: Arc<dyn ServerCredentialStore>,
    /// 活跃 SSH 连接缓存：server_id → Arc<dyn Host>。
    hosts: Arc<RwLock<HashMap<String, Arc<dyn Host>>>>,
    /// 事件总线，用于发布部署进度。
    event_bus: Arc<crate::events::BroadcastEventBus>,
    /// 活跃部署任务的取消令牌：server_id → CancellationToken。
    deploy_tasks: Arc<RwLock<HashMap<String, tokio_util::sync::CancellationToken>>>,
}

impl ServerManager {
    pub fn new(
        data_root: &Path,
        credentials: Arc<dyn ServerCredentialStore>,
        event_bus: Arc<crate::events::BroadcastEventBus>,
    ) -> Self {
        Self {
            repo: ServerProfileRepo::new(data_root),
            credentials,
            hosts: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            deploy_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn list_servers(&self) -> Vec<ServerProfile> {
        self.repo.load().await
    }

    pub async fn add_server(
        &self,
        mut profile: ServerProfile,
        password: Option<String>,
    ) -> Result<ServerProfile, String> {
        if profile.id.is_empty() {
            profile.id = short_uuid();
        }
        if let Some(pw) = &password {
            if profile.remember_credential {
                self.credentials.set_password(&profile.id, pw)?;
            }
        }
        let mut all = self.repo.load().await;
        if all.iter().any(|p| p.id == profile.id) {
            return Err(format!("server id already exists: {}", profile.id));
        }
        all.push(profile.clone());
        self.repo.save(&all).await?;
        Ok(profile)
    }

    pub async fn update_server(
        &self,
        profile: ServerProfile,
        password: Option<String>,
    ) -> Result<ServerProfile, String> {
        let mut all = self.repo.load().await;
        let pos = all
            .iter()
            .position(|p| p.id == profile.id)
            .ok_or_else(|| format!("server not found: {}", profile.id))?;
        if let Some(pw) = &password {
            if profile.remember_credential {
                self.credentials.set_password(&profile.id, pw)?;
            } else {
                let _ = self.credentials.delete_password(&profile.id);
            }
        }
        all[pos] = profile.clone();
        self.repo.save(&all).await?;
        Ok(profile)
    }

    pub async fn delete_server(&self, id: &str) -> Result<(), String> {
        let mut all = self.repo.load().await;
        let len_before = all.len();
        all.retain(|p| p.id != id);
        if all.len() == len_before {
            return Err(format!("server not found: {id}"));
        }
        self.repo.save(&all).await?;
        let _ = self.credentials.delete_password(id);
        self.hosts.write().await.remove(id);
        Ok(())
    }

    /// 测试 SSH 连接：握手 + 认证 + 执行 `uname -a` 拿 OS 信息。
    pub async fn test_connection(
        &self,
        id: &str,
        password: Option<String>,
    ) -> Result<ProbeReport, String> {
        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?
            .clone();

        let start = std::time::Instant::now();
        let credentials = self.build_credentials(&profile, password.as_deref())?;
        let config = ConnectionConfig::new(
            &profile.host,
            profile.port,
            credentials,
            HostKeyPolicy::Insecure,
        );

        let host = match RemoteLinuxHost::connect(&profile.id, config).await {
            Ok(h) => h,
            Err(err) => {
                self.update_state(id, ServerState::Failed).await;
                return Ok(ProbeReport {
                    success: false,
                    os_info: None,
                    error: Some(err.to_string()),
                    latency_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // 拿 OS 信息。
        let os_info = match host
            .run_to_string(ncd_host::HostCommand::new("uname").arg("-a"))
            .await
        {
            Ok(out) if out.success() => Some(out.stdout.trim().to_string()),
            _ => None,
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        // 缓存连接。
        self.hosts
            .write()
            .await
            .insert(profile.id.clone(), Arc::new(host));
        self.update_state(id, ServerState::Connected).await;

        Ok(ProbeReport {
            success: true,
            os_info,
            error: None,
            latency_ms,
        })
    }

    /// 获取已缓存的 Host 连接（test_connection 成功后可用）。
    pub async fn get_host(&self, id: &str) -> Option<Arc<dyn Host>> {
        self.hosts.read().await.get(id).cloned()
    }

    /// 在指定远端部署运行时组件（NapCat / SnowLuma）。
    ///
    /// 走 ncd-deploy 的 DeployPlan 编排（Component × Host × Action）。
    /// 进度通过 TaskProgress 事件广播，task_id = `deploy:<server_id>`。
    /// 可通过 `cancel_deploy` 取消。
    pub async fn deploy(
        &self,
        server_id: &str,
        flavor: ncd_domain::BotFlavor,
    ) -> Result<(), String> {
        let host = self
            .get_host(server_id)
            .await
            .ok_or_else(|| format!("主机未连接: {server_id}，请先测试连接"))?;

        let task_id = format!("deploy:{server_id}");
        let cancel = tokio_util::sync::CancellationToken::new();
        self.deploy_tasks
            .write()
            .await
            .insert(server_id.to_string(), cancel.clone());

        // 发起始进度事件。
        self.event_bus.publish(crate::events::DomainEvent::task_progress(
            &task_id, 0, "开始部署",
        ));

        // 选择 component 列表。
        use ncd_component::ActionCtx;

        // 组件工厂尚未就绪——远端安装路径推导逻辑待后续 spec 补充。
        // 当前先返回明确错误，让前端知道此功能未实装。
        let _ = cancel;
        self.deploy_tasks.write().await.remove(server_id);
        let msg = format!(
            "远端部署 {:?} 尚未实装：组件安装路径推导逻辑待补充",
            flavor
        );
        self.event_bus.publish(crate::events::DomainEvent::task_progress(
            &task_id, 0, &msg,
        ));
        Err(msg)
    }

    /// 取消正在进行的部署任务。
    pub async fn cancel_deploy(&self, server_id: &str) -> Result<(), String> {
        let token = self.deploy_tasks.read().await.get(server_id).cloned();
        match token {
            Some(t) => {
                t.cancel();
                Ok(())
            }
            None => Err(format!("没有正在进行的部署任务: {server_id}")),
        }
    }

    // ---- helpers ----

    fn build_credentials(
        &self,
        profile: &ServerProfile,
        password: Option<&str>,
    ) -> Result<SshCredentials, String> {
        match profile.auth_method {
            AuthMethod::Password => {
                let pw = password
                    .map(|s| s.to_string())
                    .or_else(|| self.credentials.get_password(&profile.id))
                    .ok_or_else(|| "密码未提供且 keyring 中无缓存".to_string())?;
                Ok(SshCredentials::password(&profile.username, &pw))
            }
            AuthMethod::Key => {
                let key_path = profile
                    .private_key_path
                    .as_deref()
                    .ok_or_else(|| "私钥路径未配置".to_string())?;
                let passphrase = password
                    .map(|s| s.to_string())
                    .or_else(|| self.credentials.get_password(&profile.id));
                Ok(SshCredentials::key_file(
                    &profile.username,
                    PathBuf::from(key_path),
                    passphrase,
                ))
            }
        }
    }

    async fn update_state(&self, id: &str, state: ServerState) {
        let mut all = self.repo.load().await;
        if let Some(p) = all.iter_mut().find(|p| p.id == id) {
            p.state = state;
            let _ = self.repo.save(&all).await;
        }
    }
}

fn short_uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.r#gen();
    hex::encode(bytes)
}

/// 按 ComponentId 查找对应的 Component 实例。
/// 远端安装需要知道 install_dir 等参数，当前返回 None 表示组件工厂尚未就绪。
/// 后续实装时由 deploy 方法根据 ServerProfile + Host 信息构造合适的 Component。
fn component_for(_id: ComponentId) -> Option<Arc<dyn Component>> {
    // 组件工厂待后续 spec 补充——NapCatComponent / SnowLumaComponent 的远端
    // 安装路径需要从 host 的运行时目录推导，不能用空值构造。
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_profile(id: &str, name: &str) -> ServerProfile {
        ServerProfile {
            id: id.to_string(),
            name: name.to_string(),
            host: "192.168.1.100".to_string(),
            port: 22,
            username: "ubuntu".to_string(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            remember_credential: true,
            state: ServerState::Disconnected,
            webui_url: None,
        }
    }

    fn make_mgr(root: &Path) -> (ServerManager, Arc<InMemoryCredentialStore>) {
        let creds = Arc::new(InMemoryCredentialStore::default());
        let bus = Arc::new(crate::events::BroadcastEventBus::default());
        let mgr = ServerManager::new(root, creds.clone(), bus);
        (mgr, creds)
    }

    #[tokio::test]
    async fn add_and_list_servers() {
        let root = tempdir().unwrap();
        let (mgr, creds) = make_mgr(root.path());

        let p = mgr
            .add_server(make_profile("s1", "My Server"), Some("pw123".into()))
            .await
            .unwrap();
        assert_eq!(p.id, "s1");

        let list = mgr.list_servers().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "My Server");

        assert_eq!(creds.get_password("s1"), Some("pw123".into()));
    }

    #[tokio::test]
    async fn add_rejects_duplicate_id() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());

        mgr.add_server(make_profile("s1", "A"), None).await.unwrap();
        let err = mgr.add_server(make_profile("s1", "B"), None).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn update_server_changes_fields() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());

        mgr.add_server(make_profile("s1", "Old"), None).await.unwrap();
        let mut updated = make_profile("s1", "New Name");
        updated.host = "10.0.0.1".to_string();
        mgr.update_server(updated, None).await.unwrap();

        let list = mgr.list_servers().await;
        assert_eq!(list[0].name, "New Name");
        assert_eq!(list[0].host, "10.0.0.1");
    }

    #[tokio::test]
    async fn delete_server_removes_and_cleans_credential() {
        let root = tempdir().unwrap();
        let (mgr, creds) = make_mgr(root.path());

        mgr.add_server(make_profile("s1", "A"), Some("secret".into()))
            .await
            .unwrap();
        assert!(creds.get_password("s1").is_some());

        mgr.delete_server("s1").await.unwrap();
        assert!(mgr.list_servers().await.is_empty());
        assert!(creds.get_password("s1").is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_error() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());
        let err = mgr.delete_server("ghost").await;
        assert!(err.is_err());
    }

    #[test]
    fn server_profile_serialization_round_trip() {
        let p = make_profile("s1", "Test");
        let json = serde_json::to_string(&p).unwrap();
        let back: ServerProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn probe_report_serialization() {
        let report = ProbeReport {
            success: true,
            os_info: Some("Linux 6.1".into()),
            error: None,
            latency_ms: 42,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("latencyMs"));
        assert!(json.contains("osInfo"));
    }
}
