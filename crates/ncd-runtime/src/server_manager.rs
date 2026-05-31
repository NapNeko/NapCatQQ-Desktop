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
use tokio::sync::{Mutex, RwLock};
use ts_rs::TS;

use ncd_host::remote::{ConnectionConfig, HostKeyPolicy, RemoteLinuxHost, SshCredentials};
use ncd_host::Host;

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
///
/// 两类凭据分开存:SSH 登录密码(account `ssh:<id>`)与 sudo 提权密码
/// (account `sudo:<id>`)。多数云主机两者相同,但密钥登录的机器只有后者,
/// 分开存才能各自独立增删。
pub trait ServerCredentialStore: Send + Sync {
    fn get_password(&self, server_id: &str) -> Option<String>;
    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String>;
    fn delete_password(&self, server_id: &str) -> Result<(), String>;

    fn get_sudo_password(&self, server_id: &str) -> Option<String>;
    fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String>;
    fn delete_sudo_password(&self, server_id: &str) -> Result<(), String>;
}

/// 基于 keyring crate 的生产实装（Windows wincred / macOS Keychain / Linux secret-service）。
pub struct KeyringCredentialStore;

/// keyring 读/写/删的公共逻辑,account 由 "<prefix>:<id>" 拼成,避免 ssh / sudo
/// 两套各写一遍(DRY)。
fn keyring_get(prefix: &str, server_id: &str) -> Option<String> {
    let account = format!("{prefix}:{server_id}");
    keyring::Entry::new(KEYRING_SERVICE, &account)
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

fn keyring_set(prefix: &str, server_id: &str, password: &str) -> Result<(), String> {
    let account = format!("{prefix}:{server_id}");
    keyring::Entry::new(KEYRING_SERVICE, &account)
        .map_err(|e| e.to_string())?
        .set_password(password)
        .map_err(|e| e.to_string())
}

fn keyring_delete(prefix: &str, server_id: &str) -> Result<(), String> {
    let account = format!("{prefix}:{server_id}");
    match keyring::Entry::new(KEYRING_SERVICE, &account) {
        Ok(entry) => {
            let _ = entry.delete_password();
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

impl ServerCredentialStore for KeyringCredentialStore {
    fn get_password(&self, server_id: &str) -> Option<String> {
        keyring_get("ssh", server_id)
    }

    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        keyring_set("ssh", server_id, password)
    }

    fn delete_password(&self, server_id: &str) -> Result<(), String> {
        keyring_delete("ssh", server_id)
    }

    fn get_sudo_password(&self, server_id: &str) -> Option<String> {
        keyring_get("sudo", server_id)
    }

    fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        keyring_set("sudo", server_id, password)
    }

    fn delete_sudo_password(&self, server_id: &str) -> Result<(), String> {
        keyring_delete("sudo", server_id)
    }
}

/// 内存 mock（测试用）。ssh / sudo 用同一张表,key 带前缀区分。
#[derive(Default)]
pub struct InMemoryCredentialStore {
    store: std::sync::Mutex<HashMap<String, String>>,
}

impl ServerCredentialStore for InMemoryCredentialStore {
    fn get_password(&self, server_id: &str) -> Option<String> {
        self.store.lock().unwrap().get(&format!("ssh:{server_id}")).cloned()
    }

    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .insert(format!("ssh:{server_id}"), password.to_string());
        Ok(())
    }

    fn delete_password(&self, server_id: &str) -> Result<(), String> {
        self.store.lock().unwrap().remove(&format!("ssh:{server_id}"));
        Ok(())
    }

    fn get_sudo_password(&self, server_id: &str) -> Option<String> {
        self.store.lock().unwrap().get(&format!("sudo:{server_id}")).cloned()
    }

    fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .insert(format!("sudo:{server_id}"), password.to_string());
        Ok(())
    }

    fn delete_sudo_password(&self, server_id: &str) -> Result<(), String> {
        self.store.lock().unwrap().remove(&format!("sudo:{server_id}"));
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
    /// 生成的免密私钥落盘目录：`<data_root>/ssh_keys/`。
    key_dir: PathBuf,
    /// 活跃 SSH 连接缓存：server_id → Arc<dyn Host>。
    hosts: Arc<RwLock<HashMap<String, Arc<dyn Host>>>>,
    /// 每服务器的连接单飞锁：server_id → Mutex。
    ///
    /// 组件页进来时会并发触发 5+ 个 detect，每个都可能在冷缓存下尝试自动连接
    /// 同一台远端。没有这把锁的话就是 5 个 SSH 握手同时砸过去，服务端
    /// MaxStartups 很容易拒掉一部分（表现为时好时坏的探测失败）。ensure_connected
    /// 抢这把锁后会二次检查缓存，等锁期间别人连上了就直接复用，真连接只发生一次。
    connect_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
}

impl ServerManager {
    pub fn new(
        data_root: &Path,
        credentials: Arc<dyn ServerCredentialStore>,
    ) -> Self {
        Self {
            repo: ServerProfileRepo::new(data_root),
            credentials,
            key_dir: data_root.join("ssh_keys"),
            hosts: Arc::new(RwLock::new(HashMap::new())),
            connect_locks: Arc::new(RwLock::new(HashMap::new())),
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
        // 连接信息可能已改（host/port/认证），丢弃缓存的旧连接，下次访问用新档案
        // 重新连。否则编辑完仍走旧 SSH 会话，改了地址也不生效。
        self.hosts.write().await.remove(&profile.id);
        self.update_state(&profile.id, ServerState::Disconnected).await;
        Ok(profile)
    }

    /// 密码登录 → 自动配置免密。
    ///
    /// 流程：用密码连一次远端 → 本地生成 ed25519 密钥对 → 把公钥追加进远端
    /// `~/.ssh/authorized_keys`（去重，已存在则不重复加）→ 私钥落盘到
    /// `<data_root>/ssh_keys/<id>` → 档案切到 Key 认证、指向该私钥。之后连接
    /// 走密钥免密，不再需要密码。
    ///
    /// 失败保持档案原样（仍是密码认证），返回人话错误。
    pub async fn setup_key_auth(&self, id: &str, password: &str) -> Result<ServerProfile, String> {
        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?
            .clone();

        // 1. 用密码连一次（不复用缓存，确保是密码通道）。
        let credentials = SshCredentials::password(&profile.username, password);
        let config = ConnectionConfig::new(
            &profile.host,
            profile.port,
            credentials,
            HostKeyPolicy::Insecure,
        );
        let host = RemoteLinuxHost::connect(&profile.id, config)
            .await
            .map_err(|e| format!("密码连接失败: {e}（请检查用户名 / 密码 / 网络）"))?;

        // 2. 本地生成密钥对。
        let comment = format!("napcatqq-desktop@{}", profile.id);
        let pair = crate::ssh_keygen::generate_ed25519(&comment)?;

        // 3. 公钥追加进远端 authorized_keys（幂等：先 grep 去重）。
        //    用 sh 单行：建 ~/.ssh（700）→ 若公钥不在 authorized_keys 就追加 →
        //    authorized_keys 设 600。authorized_keys 行用单引号包，避免 shell 解释。
        let pub_line = pair.public_line.trim();
        let script = format!(
            "set -e; mkdir -p ~/.ssh && chmod 700 ~/.ssh; \
             touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys; \
             grep -qxF '{pub}' ~/.ssh/authorized_keys || echo '{pub}' >> ~/.ssh/authorized_keys",
            pub = pub_line,
        );
        let out = host
            .run_to_string(ncd_host::HostCommand::new("sh").arg("-c").arg(script))
            .await
            .map_err(|e| format!("写入远端 authorized_keys 失败: {e}"))?;
        if !out.success() {
            return Err(format!(
                "写入远端 authorized_keys 失败（exit={:?}）: {}",
                out.exit_code,
                out.stderr.trim()
            ));
        }

        // 4. 私钥落盘到 <data_root>/ssh_keys/<id>，权限 600（best-effort）。
        tokio::fs::create_dir_all(&self.key_dir)
            .await
            .map_err(|e| format!("创建密钥目录失败: {e}"))?;
        let key_path = self.key_dir.join(&profile.id);
        tokio::fs::write(&key_path, pair.private_openssh.as_bytes())
            .await
            .map_err(|e| format!("写入私钥失败: {e}"))?;
        set_key_file_permissions(&key_path).await;

        // 5. 档案切到 Key 认证；SSH 登录不再需要密码,清掉 ssh 凭据避免残留。
        //    但把这个登录密码挪存到 sudo 槽:绝大多数云主机 sudo 密码就是登录密码,
        //    切成密钥登录后若不留着,远端装 docker 等提权操作就只能再弹框问一次。
        //    这正是"密码登录 -> 自动配密钥后仍能找到密码"的来源。
        let key_path_str = key_path.to_string_lossy().into_owned();
        let mut updated = profile.clone();
        updated.auth_method = AuthMethod::Key;
        updated.private_key_path = Some(key_path_str);
        let _ = self.credentials.set_sudo_password(&profile.id, password);
        let _ = self.credentials.delete_password(&profile.id);

        let mut persisted = self.repo.load().await;
        if let Some(slot) = persisted.iter_mut().find(|p| p.id == id) {
            *slot = updated.clone();
            self.repo.save(&persisted).await?;
        }

        // 6. 缓存这次连接（密码通道已建立，可直接复用），状态置已连接。
        self.hosts
            .write()
            .await
            .insert(profile.id.clone(), Arc::new(host));
        self.update_state(id, ServerState::Connected).await;

        Ok(updated)
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
        let _ = self.credentials.delete_sudo_password(id);
        self.hosts.write().await.remove(id);
        Ok(())
    }

    /// 取某服务器可用于 sudo 提权的密码,给 docker 安装等提权操作用。
    /// 优先专门的 sudo 槽(密钥登录机器在这);没有就退回 SSH 登录密码(密码
    /// 登录机器 sudo 密码通常与登录密码相同)。两个都没有返回 None。
    pub fn sudo_password(&self, id: &str) -> Option<String> {
        self.credentials
            .get_sudo_password(id)
            .or_else(|| self.credentials.get_password(id))
    }

    /// 记住某服务器的 sudo 密码(用户在弹框勾了"记住密码"时调用)。
    pub fn remember_sudo_password(&self, id: &str, password: &str) -> Result<(), String> {
        self.credentials.set_sudo_password(id, password)
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

    /// 确保某服务器已连接，返回缓存的 Host。
    ///
    /// 单飞语义：先查缓存命中直接返回；未命中时抢该服务器的连接锁，再查一次
    /// 缓存（等锁期间别的并发请求可能已经连上），仍没有才用 keyring 缓存凭据
    /// 真连一次。这样组件页并发触发的 N 个 detect 只会产生一次实际 SSH 握手，
    /// 其余复用同一条连接，避免把远端 SSH 的 MaxStartups 打爆。
    ///
    /// 失败返回人话错误，调用方把它显示在对应 host 那行。
    pub async fn ensure_connected(&self, id: &str) -> Result<Arc<dyn Host>, String> {
        if let Some(host) = self.get_host(id).await {
            return Ok(host);
        }

        let lock = self.connect_lock_for(id).await;
        let _guard = lock.lock().await;

        // 二次检查：等锁期间可能已有并发请求把连接建好并缓存。
        if let Some(host) = self.get_host(id).await {
            return Ok(host);
        }

        match self.test_connection(id, None).await {
            Ok(report) if report.success => self
                .get_host(id)
                .await
                .ok_or_else(|| format!("自动连接成功但缓存为空: {id}（不应发生）")),
            Ok(report) => {
                let err = report.error.unwrap_or_else(|| "未知错误".into());
                Err(format!("自动连接失败: {err}（请去远端页手动测试连接）"))
            }
            Err(err) => Err(format!(
                "自动连接被拒绝: {err}（凭据可能未保存，请去远端页手动测试）"
            )),
        }
    }

    /// 取（或惰性创建）某服务器的连接单飞锁。
    async fn connect_lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        if let Some(lock) = self.connect_locks.read().await.get(id) {
            return Arc::clone(lock);
        }
        let mut map = self.connect_locks.write().await;
        Arc::clone(map.entry(id.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))))
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

/// 给落盘的私钥文件设权限。Unix 设 600（仅属主可读写，否则 ssh 会拒用）；
/// Windows 上文件权限模型不同，依赖 NTFS ACL 继承用户目录权限，这里 no-op。
async fn set_key_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = tokio::fs::metadata(path).await {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = tokio::fs::set_permissions(path, perms).await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn short_uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.r#gen();
    hex::encode(bytes)
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
        let mgr = ServerManager::new(root, creds.clone());
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
