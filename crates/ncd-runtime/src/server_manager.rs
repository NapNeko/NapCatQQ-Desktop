//! ServerManager:远端主机档案管理 + 凭据存储 + 连接测试
//!
//! 这个模块负责:
//! 1. ServerProfile CRUD(JSON 持久化到 <data_root>/config/servers.json)
//! 2. 凭据(密码 / 私钥密码)走 keyring 系统凭据库,不落盘
//! 3. 连接测试(SSH 握手 + 基本信息探测)
//! 4. 活跃 Host 连接缓存
//!
//! 不负责部署编排(那是 Deployment trait 的事)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ncd_domain::AppSettings;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use ts_rs::TS;

use ncd_host::remote::{
    ConnectionConfig, HostKeyCheck, HostKeyPolicy, KnownHostsStore, RemoteLinuxHost, SshCredentials,
};
use ncd_host::{Host, HostError};

use crate::credential_sync::{CredentialSyncLayer, PasswordSlot};
use crate::events::EventBus;

// ============================================================
// 数据结构
// ============================================================

/// 远端主机档案不含密码——凭据走 keyring
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ServerProfile {
    /// 内部 id,创建时生成的短 UUID
    pub id: String,
    /// 用户给的显示名称
    pub name: String,
    /// 主机地址(IP 或域名)
    pub host: String,
    /// SSH 端口,默认 22
    #[serde(default = "default_port")]
    pub port: u16,
    /// 登录用户名
    pub username: String,
    /// 认证方式
    #[serde(default)]
    pub auth_method: AuthMethod,
    /// 私钥文件路径(仅 Key 方式使用)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// 用户是否选择了"记住密码"
    #[serde(default)]
    pub remember_credential: bool,
    /// 最近一次连接测试结果
    #[serde(default)]
    pub state: ServerState,
	    /// 连接健康度细粒度信息(最近成功时间,连续失败计数等)
	    /// 可选 + 默认 + 序列化时 None 省略,保证向后兼容
	    #[serde(default, skip_serializing_if = "Option::is_none")]
	    pub health: Option<ConnectionHealth>,
    /// WebUI 端点 URL(用户手填的远端 NapCat WebUI 地址)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_url: Option<String>,
}

/// 认证方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AuthMethod {
    /// 私钥认证(推荐)
    #[default]
    Key,
    /// 密码认证
    Password,
}

/// 主机连接状态
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ServerState {
    /// 未连接 / 未测试过
    #[default]
    Disconnected,
    /// 连接中
    Connecting,
    /// 连接成功
    Connected,
    /// 连接失败
    Failed,
}

	/// 连接健康度细粒度信息(可选,向后兼容)
	/// 与 ServerState(粗状态)正交:state 仍表示 Connected/Disconnected/Failed 等,
	/// health 提供最近成功时间,连续失败计数,最近失败原因等,用于前端展示和抑制策略
	#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
	#[serde(rename_all = "camelCase")]
	#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
	pub struct ConnectionHealth {
	    /// 最近一次成功连接/探测的时间(ISO8601 或毫秒时间戳字符串)
	    #[serde(default, skip_serializing_if = "Option::is_none")]
	    pub last_success_at: Option<String>,
	    /// 连续失败次数(成功后归零)
	    #[serde(default)]
	    pub consecutive_failures: u32,
	    /// 最近一次失败的原因(简短人话)
	    #[serde(default, skip_serializing_if = "Option::is_none")]
	    pub last_failure_reason: Option<String>,
	    /// 最近一次失败的时间
	    #[serde(default, skip_serializing_if = "Option::is_none")]
	    pub last_failure_at: Option<String>,
	}

/// test_connection 返回的探测报告
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
    /// 首次连接遇到未记录的 host key:连接已被阻断,前端应展示指纹让用户确认,
    /// 确认后调 confirm_host_key 写入 known_hosts 再重试非 None 不代表认证失败
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key_prompt: Option<HostKeyPrompt>,
    /// 该主机已有 known_hosts 记录但本次 key 不一致(疑似中间人)前端必须按危险
    /// 提示阻断,不得提供"一键信任",需用户人工核实
    #[serde(default)]
    pub host_key_mismatch: bool,
}

/// 待用户确认的远端 host key 指纹host key 校验走 TOFU:首次连接把指纹摆给
/// 用户,确认后才写入 known_hosts绝不在未校验的通道上写 authorized_keys 或
/// 缓存连接,避免首次连接被中间人窃取凭据
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct HostKeyPrompt {
    pub host: String,
    pub port: u16,
    /// key 算法名,如 ssh-ed25519 / rsa-sha2-512
    pub key_kind: String,
    /// base64 编码的原始公钥(写 known_hosts 用,前端原样回传 confirm_host_key)
    pub key_b64: String,
    /// 供用户核对的指纹,OpenSSH 风格 SHA256:<base64-no-pad>
    pub fingerprint: String,
}

fn default_port() -> u16 {
    22
}

// ============================================================
// 凭据存储
// ============================================================

const KEYRING_SERVICE: &str = "napcatqq-desktop";

/// 系统凭据库操作测试时可 mock
///
/// 两类凭据分开存:SSH 登录密码(account ssh:<id>)与 sudo 提权密码
/// (account sudo:<id>)多数云主机两者相同,但密钥登录的机器只有后者,
/// 分开存才能各自独立增删
pub trait ServerCredentialStore: Send + Sync {
    fn get_password(&self, server_id: &str) -> Option<String>;
    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String>;
    fn delete_password(&self, server_id: &str) -> Result<(), String>;

    fn get_sudo_password(&self, server_id: &str) -> Option<String>;
    fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String>;
    fn delete_sudo_password(&self, server_id: &str) -> Result<(), String>;
}

/// 基于 keyring crate 的生产实装(Windows wincred / macOS Keychain / Linux secret-service)
pub struct KeyringCredentialStore;

/// keyring 读/写/删的公共逻辑,account 由 "<prefix>:<id>" 拼成,避免 ssh / sudo
/// 两套各写一遍(DRY)
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

/// 内存 mock(测试用)ssh / sudo 用同一张表,key 带前缀区分
#[derive(Default)]
pub struct InMemoryCredentialStore {
    store: std::sync::Mutex<HashMap<String, String>>,
}

impl ServerCredentialStore for InMemoryCredentialStore {
    fn get_password(&self, server_id: &str) -> Option<String> {
        self.store.lock().ok()?.get(&format!("ssh:{server_id}")).cloned()
    }

    fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.store
            .lock()
            .map_err(|e| e.to_string())?
            .insert(format!("ssh:{server_id}"), password.to_string());
        Ok(())
    }

    fn delete_password(&self, server_id: &str) -> Result<(), String> {
        self.store.lock().map_err(|e| e.to_string())?.remove(&format!("ssh:{server_id}"));
        Ok(())
    }

    fn get_sudo_password(&self, server_id: &str) -> Option<String> {
        self.store.lock().ok()?.get(&format!("sudo:{server_id}")).cloned()
    }

    fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.store
            .lock()
            .map_err(|e| e.to_string())?
            .insert(format!("sudo:{server_id}"), password.to_string());
        Ok(())
    }

    fn delete_sudo_password(&self, server_id: &str) -> Result<(), String> {
        self.store.lock().map_err(|e| e.to_string())?.remove(&format!("sudo:{server_id}"));
        Ok(())
    }
}

// ============================================================
// ServerProfileRepo:JSON 持久化
// ============================================================

/// servers.json 路径固定在 <data_root>/config/servers.json
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
    sync: CredentialSyncLayer,
    /// 生成的免密私钥落盘目录:<data_root>/ssh_keys/
    key_dir: PathBuf,
    /// TOFU host key 数据库路径:<data_root>/secrets/known_hosts生产 SSH 连接
    /// 用 AcceptOnFirstUse 策略校验 host key,未知主机要用户确认后写到这里
    known_hosts_path: PathBuf,
    /// 活跃 SSH 连接缓存:server_id → Arc<dyn Host>
    hosts: Arc<RwLock<HashMap<String, Arc<dyn Host>>>>,
    /// 每服务器的连接单飞锁:server_id → Mutex
    ///
    /// 组件页进来时会并发触发 5+ 个 detect,每个都可能在冷缓存下尝试自动连接
    /// 同一台远端没有这把锁的话就是 5 个 SSH 握手同时砸过去,服务端
    /// MaxStartups 很容易拒掉一部分(表现为时好时坏的探测失败)ensure_connected
    /// 抢这把锁后会二次检查缓存,等锁期间别人连上了就直接复用,真连接只发生一次
    connect_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    /// 自动连接失败后冷却,避免 detect 轮询刷 SSH 失败日志
    auto_connect_cooldown_until: Arc<RwLock<HashMap<String, std::time::Instant>>>,
    /// 可选的事件总线,用于发布 HostConnectionLost / HostConnectionRecovered
    /// 由 Tauri 侧 wiring 时通过 set_event_bus 注入;未注入时不发事件(向后兼容)
    event_sink: Option<Arc<dyn EventBus>>,
}

impl ServerManager {
    pub fn new(
        data_root: &Path,
        credentials: Arc<dyn ServerCredentialStore>,
    ) -> Self {
        Self {
            repo: ServerProfileRepo::new(data_root),
            sync: CredentialSyncLayer::new(credentials),
            key_dir: data_root.join("ssh_keys"),
            known_hosts_path: data_root.join("secrets").join("known_hosts"),
            hosts: Arc::new(RwLock::new(HashMap::new())),
            connect_locks: Arc::new(RwLock::new(HashMap::new())),
            auto_connect_cooldown_until: Arc::new(RwLock::new(HashMap::new())),
            event_sink: None,
        }
    }

    /// 生产 SSH 连接的 host key 策略:TOFU(AcceptOnFirstUse)首次未知主机返回
    /// HostKeyUnknown(连接被阻断,等用户确认指纹后写 known_hosts);已记录但 key
    /// 变了返回 HostKeyMismatch 直接阻断永不使用 Insecure——那会让首次连接的
    /// 中间人窃取登录密码,并把攻击者公钥写进远端 authorized_keys
    fn host_key_policy(&self) -> HostKeyPolicy {
        HostKeyPolicy::AcceptOnFirstUse {
            known_hosts_path: self.known_hosts_path.clone(),
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
                self.sync.credentials().set_password(&profile.id, pw)?;
            }
        }
        let mut all = self.repo.load().await;
        if all.iter().any(|p| p.id == profile.id) {
            return Err(format!("server id already exists: {}", profile.id));
        }
        all.push(profile.clone());
        self.repo.save(&all).await?;
        info!(
            target: "ncd_runtime::server_manager",
            server_id = %profile.id,
            host = %profile.host,
            port = profile.port,
            "远端主机已加入档案"
        );
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

        let changed_slot = if let Some(pw) = &password {
            if profile.remember_credential {
                self.sync.credentials().set_password(&profile.id, pw)?;
                // 关键修复:同时更新 sudo 槽,确保提权操作可用
                let _ = self.sync.credentials().set_sudo_password(&profile.id, pw);
                Some(PasswordSlot::Ssh)
            } else {
                let _ = self.sync.credentials().delete_password(&profile.id);
                let _ = self.sync.credentials().delete_sudo_password(&profile.id);
                None
            }
        } else {
            None
        };

        all[pos] = profile.clone();
        self.repo.save(&all).await?;

        // 关键改进:SSH 密码变了必须清缓存,sudo 密码变了可以热更新
        if let Some(slot) = changed_slot {
            if self.sync.on_password_changed(&profile.id, slot) {
                self.hosts.write().await.remove(&profile.id);
                self.update_state(&profile.id, ServerState::Disconnected).await;

                // 立即尝试静默重连(利用新凭据)
                if profile.remember_credential {
                    let _ = self.ensure_connected(&profile.id).await;
                }
            } else {
                // sudo 密码变更:热更新缓存连接
                if let Some(cached) = self.hosts.read().await.get(&profile.id) {
                    self.sync.sync_elevation_to_host(&profile.id, cached.as_ref()).await;
                }
            }
        } else {
            // 连接信息可能已改(host/port/认证),丢弃缓存的旧连接
            self.hosts.write().await.remove(&profile.id);
            self.update_state(&profile.id, ServerState::Disconnected).await;
        }

        Ok(profile)
    }

    /// 密码登录 → 自动配置免密
    ///
    /// 流程:用密码连一次远端 → 本地生成 ed25519 密钥对 → 把公钥追加进远端
    /// ~/.ssh/authorized_keys(去重,已存在则不重复加)→ 私钥落盘到
    /// <data_root>/ssh_keys/<id> → 档案切到 Key 认证,指向该私钥之后连接
    /// 走密钥免密,不再需要密码
    ///
    /// 失败保持档案原样(仍是密码认证),返回人话错误
    pub async fn setup_key_auth(&self, id: &str, password: &str) -> Result<ServerProfile, String> {
        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?
            .clone();

        // 1. 用密码连一次(不复用缓存,确保是密码通道)
        //    host key 走 TOFU:首次未知主机会被阻断,提示用户先在该服务器点「测试
        //    连接」确认指纹(写进 known_hosts)后再配免密——绝不在未校验通道上把公钥
        //    写进远端 authorized_keys,否则首次连接中间人能窃取密码并植入自己的 key
        let credentials = SshCredentials::password(&profile.username, password);
        let config = ConnectionConfig::new(
            &profile.host,
            profile.port,
            credentials,
            self.host_key_policy(),
        );
        let host = RemoteLinuxHost::connect(&profile.id, config)
            .await
            .map_err(|e| match &e {
                HostError::HostKeyUnknown { .. } => {
                    "远端 host key 尚未确认。请先在该服务器上点「测试连接」,核对并信任指纹后再配置免密。"
                        .to_string()
                }
                HostError::HostKeyMismatch { .. } => {
                    "远端 host key 与已记录的不一致,疑似中间人攻击,已阻断。请人工核实后再试。"
                        .to_string()
                }
                _ => format!("密码连接失败: {e}（请检查用户名 / 密码 / 网络）"),
            })?;

        // 2. 本地生成密钥对
        let comment = format!("napcatqq-desktop@{}", profile.id);
        let pair = crate::ssh_keygen::generate_ed25519(&comment)?;

        // 3. 公钥追加进远端 authorized_keys公钥从 stdin 传入,避免把 key 文本拼进 shell
        let pub_line = pair.public_line.trim();
        let script = "set -eu\n\
            umask 077\n\
            mkdir -p \"$HOME/.ssh\"\n\
            touch \"$HOME/.ssh/authorized_keys\"\n\
            chmod 700 \"$HOME/.ssh\"\n\
            chmod 600 \"$HOME/.ssh/authorized_keys\"\n\
            IFS= read -r pub_key\n\
            grep -qxF -- \"$pub_key\" \"$HOME/.ssh/authorized_keys\" || \
            printf '%s\\n' \"$pub_key\" >> \"$HOME/.ssh/authorized_keys\"\n";
        let out = host
            .run_to_string(
                ncd_host::HostCommand::new("sh")
                    .arg("-c")
                    .arg(script)
                    .stdin(format!("{pub_line}\n").into_bytes()),
            )
            .await
            .map_err(|e| format!("写入远端 authorized_keys 失败: {e}"))?;
        if !out.success() {
            return Err(format!(
                "写入远端 authorized_keys 失败（exit={:?}）: {}",
                out.exit_code,
                out.stderr.trim()
            ));
        }

        // 4. 私钥落盘到 <data_root>/ssh_keys/<id>,权限 600(best-effort)
        tokio::fs::create_dir_all(&self.key_dir)
            .await
            .map_err(|e| format!("创建密钥目录失败: {e}"))?;
        let key_path = self.key_dir.join(&profile.id);
        tokio::fs::write(&key_path, pair.private_openssh.as_bytes())
            .await
            .map_err(|e| format!("写入私钥失败: {e}"))?;
        set_key_file_permissions(&key_path).await;

        // 5. 档案切到 Key 认证;SSH 登录不再需要密码,清掉 ssh 凭据避免残留
        //    但把这个登录密码挪存到 sudo 槽:绝大多数云主机 sudo 密码就是登录密码,
        //    切成密钥登录后若不留着,远端装 docker 等提权操作就只能再弹框问一次
        //    这正是"密码登录 -> 自动配密钥后仍能找到密码"的来源
        let key_path_str = key_path.to_string_lossy().into_owned();
        let mut updated = profile.clone();
        updated.auth_method = AuthMethod::Key;
        updated.private_key_path = Some(key_path_str);

        // 关键改进:使用 sync 层的迁移方法(显式语义)
        self.sync.migrate_ssh_to_sudo(&profile.id)?;
        let _ = self.sync.credentials().delete_password(&profile.id);

        let mut persisted = self.repo.load().await;
        if let Some(slot) = persisted.iter_mut().find(|p| p.id == id) {
            *slot = updated.clone();
            self.repo.save(&persisted).await?;
        }

        // 6. 缓存这次连接(密码通道已建立,可直接复用),状态置已连接
        //    顺手把这次的登录密码注入 host 当提权密码——刚挪存到 sudo 槽的就是它,
        //    省得密钥登录后第一次 elevated 操作还得回 keyring 取
        let host: Arc<dyn Host> = Arc::new(host);
        host.set_elevation_password(Some(password.to_string())).await;
        self.hosts
            .write()
            .await
            .insert(profile.id.clone(), host);
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
        info!(target: "ncd_runtime::server_manager", server_id = %id, "远端主机档案已删除");
        let _ = self.sync.credentials().delete_password(id);
        let _ = self.sync.credentials().delete_sudo_password(id);
        self.purge_server_runtime_maps(id).await;
        Ok(())
    }

    /// 档案删除时清 SSH 缓存,单飞锁与自动连冷却表,避免 Map 只增不减
    async fn purge_server_runtime_maps(&self, id: &str) {
        self.hosts.write().await.remove(id);
        self.connect_locks.write().await.remove(id);
        self.auto_connect_cooldown_until.write().await.remove(id);
    }

    /// 冷却条目到期后仍留在 HashMap 里会造成慢泄漏;读路径顺手删掉已过期的键
    async fn prune_expired_auto_connect_cooldowns(&self) {
        let now = std::time::Instant::now();
        self.auto_connect_cooldown_until
            .write()
            .await
            .retain(|_, until| *until > now);
    }

    /// 为长操作(Docker 安装,组件安装等)创建隔离的 SSH 连接
    ///
    /// 银弹设计:长操作可能污染 SSH 会话环境(sudo 缓存,环境变量,shell 状态),
    /// 用独立连接隔离,操作完成后连接自动丢弃,不影响缓存池
    ///
    /// 短操作(探测,列文件)仍使用 ensure_connected 的缓存连接,保持性能
    pub async fn with_isolated_connection<F, T>(&self, id: &str, f: F) -> Result<T, String>
    where
        F: FnOnce(Arc<dyn Host>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>,
    {
        info!(
            target: "ncd_runtime::server_manager",
            server_id = %id,
            "为长操作建立隔离 SSH 连接"
        );

        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?
            .clone();

        let credentials = self.build_credentials(&profile, None)?;
        let config = ConnectionConfig::new(
            &profile.host,
            profile.port,
            credentials,
            self.host_key_policy(),
        )
        .with_keepalive(Some(std::time::Duration::from_secs(15)));

        let host = RemoteLinuxHost::connect(&profile.id, config)
            .await
            .map_err(|e| format!("隔离连接建立失败: {e}"))?;

        let host: Arc<dyn Host> = Arc::new(host);

        // 关键改进:立即同步最新密码(解决缓存过期问题)
        self.sync.sync_elevation_to_host(id, host.as_ref()).await;

        let result = f(host.clone()).await;

        info!(
            target: "ncd_runtime::server_manager",
            server_id = %id,
            "隔离连接已关闭"
        );

        result
    }

    /// 取某服务器可用于 sudo 提权的密码,给 docker 安装等提权操作用
    /// 优先专门的 sudo 槽(密钥登录机器在这);没有就退回 SSH 登录密码(密码
    /// 登录机器 sudo 密码通常与登录密码相同)两个都没有返回 None
    pub fn sudo_password(&self, id: &str) -> Option<String> {
        self.sync.elevation_password(id)
    }

    /// 记住某服务器的 sudo 密码(用户在弹框勾了"记住密码"时调用)下次该服务器
    /// 连接(或重连)时 inject_elevation_password 会把它注入 host
    pub fn remember_sudo_password(&self, id: &str, password: &str) -> Result<(), String> {
        self.sync.remember_sudo(id, password)
    }

    /// 把 keyring 里这台服务器的提权密码注入一条已建立的 host 连接没有任何缓存
    /// 密码时注入 None(host 退回 sudo -n,免密/root 仍能跑)test_connection
    /// 缓存连接后调用,让这条 host 后续所有 elevated 操作自动带上密码
    async fn inject_elevation_password(&self, id: &str, host: &dyn Host) {
        self.sync.sync_elevation_to_host(id, host).await;
    }

    /// 测试 SSH 连接:握手 + 认证 + 执行 uname -a 拿 OS 信息
    ///
    /// log_probe:为 false 时不写「正在测试 SSH」类 INFO(ensure_connected 自动重试用)
    pub async fn test_connection(
        &self,
        id: &str,
        password: Option<String>,
        log_probe: bool,
    ) -> Result<ProbeReport, String> {
        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?
            .clone();

        let start = std::time::Instant::now();
        if log_probe {
            info!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                host = %profile.host,
                port = profile.port,
                "正在测试 SSH 连接"
            );
        }
        let credentials = self.build_credentials(&profile, password.as_deref())?;
        let config = ConnectionConfig::new(
            &profile.host,
            profile.port,
            credentials,
            self.host_key_policy(),
        );

        let host = match RemoteLinuxHost::connect(&profile.id, config).await {
            Ok(h) => h,
            Err(err) => {
                self.update_state(id, ServerState::Failed).await;

                // P0-10: 更新 health 失败信息 + 递增连续失败计数 + 发布 lost 事件
                let latency_ms = start.elapsed().as_millis() as u64;
                let now = chrono::Utc::now().to_rfc3339();
                let err_text = err.to_string();
                let mut consecutive = 0u32;
                self.update_health_fields(id, |h| {
                    h.last_failure_reason = Some(err_text.clone());
                    h.last_failure_at = Some(now.clone());
                    h.consecutive_failures = h.consecutive_failures.saturating_add(1);
                    consecutive = h.consecutive_failures;
                }).await;
                self.publish_host_lost(id, Some(err_text), consecutive);

                let (error, host_key_prompt, host_key_mismatch) = classify_connect_error(&err);
                if log_probe {
                    tracing::warn!(
                        target: "ncd_runtime::server_manager",
                        server_id = %id,
                        host = %profile.host,
                        err = %error,
                        "SSH 连接测试失败"
                    );
                }
                return Ok(ProbeReport {
                    success: false,
                    os_info: None,
                    error: Some(error),
                    latency_ms,
                    host_key_prompt,
                    host_key_mismatch,
                });
            }
        };

        // 拿 OS 信息
        let os_info = match host
            .run_to_string(ncd_host::HostCommand::new("uname").arg("-a"))
            .await
        {
            Ok(out) if out.success() => Some(out.stdout.trim().to_string()),
            _ => None,
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        // 缓存连接,并把 keyring 里的提权密码注入这条 host:之后任何 elevated 操作
        // (装 unzip,写 /opt/QQ,apt 装包,装 docker)都自动用上,不必每条命令各传
        let host: Arc<dyn Host> = Arc::new(host);

        // 关键改进:自动同步 ssh 密码到 sudo 槽(setup_key_auth 之前的平滑迁移)
        if let Err(e) = self.sync.migrate_ssh_to_sudo(id) {
            tracing::warn!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                err = %e,
                "SSH 密码迁移到 sudo 槽失败，后续提权操作可能需要重新输入密码"
            );
        }

        self.inject_elevation_password(id, host.as_ref()).await;
        self.hosts
            .write()
            .await
            .insert(profile.id.clone(), host);
        self.update_state(id, ServerState::Connected).await;

        // P0-10: 更新 health 成功时间 + 归零失败计数 + 发布恢复事件
        let now = chrono::Utc::now().to_rfc3339();
        self.update_health_fields(id, |h| {
            h.last_success_at = Some(now.clone());
            h.consecutive_failures = 0;
        }).await;
        self.publish_host_recovered(id, latency_ms);

        if log_probe {
            info!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                latency_ms,
                os = os_info.as_deref().unwrap_or(""),
                "SSH 连接测试成功"
            );
        }

        Ok(ProbeReport {
            success: true,
            os_info,
            error: None,
            latency_ms,
            host_key_prompt: None,
            host_key_mismatch: false,
        })
    }

    /// 用户在指纹确认弹窗点"信任"后调用:把这条 host key 写进 known_hosts,之后该
    /// 主机的连接(test / 配免密 / 自动重连)即可通过 TOFU 校验
    ///
    /// 安全约束:只在该主机当前"未知"时才追加若 known_hosts 里已有同主机但 key
    /// 不同(mismatch),拒绝写入并报错——这种情况是疑似中间人或服务端换了 key,必须
    /// 用户人工核实后手动清理 known_hosts,不能在产品里一键覆盖
    pub async fn confirm_host_key(
        &self,
        id: &str,
        key_kind: &str,
        key_b64: &str,
    ) -> Result<(), String> {
        let all = self.repo.load().await;
        let profile = all
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("server not found: {id}"))?;

        let store = KnownHostsStore::new(self.known_hosts_path.clone());
        match store
            .check(&profile.host, profile.port, key_kind, key_b64)
            .await
            .map_err(|e| format!("读取 known_hosts 失败: {e}"))?
        {
            HostKeyCheck::Match => Ok(()),
            HostKeyCheck::Mismatch => Err(
                "该主机已记录了不同的 host key,疑似中间人或服务端更换密钥。出于安全已拒绝自动信任,\
                 请人工核实后再手动清理 known_hosts。"
                    .to_string(),
            ),
            HostKeyCheck::Unknown => store
                .append(&profile.host, profile.port, key_kind, key_b64)
                .await
                .map_err(|e| format!("写入 known_hosts 失败: {e}")),
        }
    }

    /// 获取已缓存的 Host 连接(test_connection 成功后可用)
    pub async fn get_host(&self, id: &str) -> Option<Arc<dyn Host>> {
        self.hosts.read().await.get(id).cloned()
    }

    /// 丢弃缓存中的 SSH 连接(会话已断或不可信时调用)
    /// 下次 ensure_connected 会重新握手,避免继续复用死连接
    ///
    /// P0-10: 同时更新 health(递增失败计数 + 记原因),并发布 HostConnectionLost 事件
    pub async fn disconnect_cached_host(&self, id: &str) {
        if self.hosts.write().await.remove(id).is_some() {
            self.update_state(id, ServerState::Disconnected).await;

            // P0-10: health 失败分支 + 发布 lost
            let now = chrono::Utc::now().to_rfc3339();
            let mut consecutive = 0u32;
            self.update_health_fields(id, |h| {
                h.last_failure_reason = Some("缓存连接被显式断开".to_string());
                h.last_failure_at = Some(now.clone());
                h.consecutive_failures = h.consecutive_failures.saturating_add(1);
                consecutive = h.consecutive_failures;
            }).await;
            self.publish_host_lost(id, Some("缓存连接被显式断开".to_string()), consecutive);

            info!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                "已清除失效的远端 SSH 缓存连接"
            );
        }
    }

    /// 确保某服务器已连接,返回缓存的 Host
    ///
    /// 单飞语义:先查缓存命中直接返回;未命中时抢该服务器的连接锁,再查一次
    /// 缓存(等锁期间别的并发请求可能已经连上),仍没有才用 keyring 缓存凭据
    /// 真连一次这样组件页并发触发的 N 个 detect 只会产生一次实际 SSH 握手,
    /// 其余复用同一条连接,避免把远端 SSH 的 MaxStartups 打爆
    ///
    /// 失败返回人话错误,调用方把它显示在对应 host 那行
    pub async fn ensure_connected(&self, id: &str) -> Result<Arc<dyn Host>, String> {
        if let Some(host) = self.get_host(id).await {
            return Ok(host);
        }

        self.prune_expired_auto_connect_cooldowns().await;

        const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(90);
        if let Some(until) = self.auto_connect_cooldown_until.read().await.get(id).copied() {
            if until > std::time::Instant::now() {
                return Err(
                    "远端尚未连接（请去远端页测试连接）；近期自动连接失败，已暂停自动重试"
                        .to_string(),
                );
            }
        }

        info!(
            target: "ncd_runtime::server_manager",
            server_id = %id,
            "远端未缓存连接，正在自动建立 SSH（组件探测/安装会复用此连接）"
        );
        let lock = self.connect_lock_for(id).await;
        let _guard = lock.lock().await;

        // 二次检查:等锁期间可能已有并发请求把连接建好并缓存
        if let Some(host) = self.get_host(id).await {
            return Ok(host);
        }

        match self.test_connection(id, None, false).await {
            Ok(report) if report.success => {
                self.auto_connect_cooldown_until.write().await.remove(id);
                self.get_host(id)
                    .await
                    .ok_or_else(|| format!("自动连接成功但缓存为空: {id}（不应发生）"))
            }
            Ok(report) => {
                let err = report.error.unwrap_or_else(|| "未知错误".into());
                self.auto_connect_cooldown_until
                    .write()
                    .await
                    .insert(id.to_string(), std::time::Instant::now() + COOLDOWN);
                tracing::warn!(
                    target: "ncd_runtime::server_manager",
                    server_id = %id,
                    err = %err,
                    "远端自动连接失败"
                );
                Err(format!("自动连接失败: {err}（请去远端页手动测试连接）"))
            }
            Err(err) => {
                self.auto_connect_cooldown_until
                    .write()
                    .await
                    .insert(id.to_string(), std::time::Instant::now() + COOLDOWN);
                tracing::warn!(
                    target: "ncd_runtime::server_manager",
                    server_id = %id,
                    err = %err,
                    "远端自动连接被拒绝"
                );
                Err(format!(
                    "自动连接被拒绝: {err}（凭据可能未保存，请去远端页手动测试）"
                ))
            }
        }
    }

    /// 取(或惰性创建)某服务器的连接单飞锁
    async fn connect_lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        if let Some(lock) = self.connect_locks.read().await.get(id) {
            return Arc::clone(lock);
        }
        let mut map = self.connect_locks.write().await;
        Arc::clone(map.entry(id.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))))
    }

    // ============================================================
    // P0-10: 自愈闭环新增 API(get_live_host / refresh_host / mark_unhealthy)
    // ============================================================

    /// 取"当前应存活"的 host
    ///
    /// - 缓存未命中 → 走 ensure_connected(含单飞 + 自动连 + 冷却)
    /// - 缓存命中 → 先做廉价活性探测(is_healthy),成功则返回;失败则 mark_unhealthy + 驱逐 + 走 ensure_connected 重连
    /// - 全程受单飞保护(复用/扩展 connect_locks)
    ///
    /// 语义:调用方可信返回的连接在本方法返回时刻是可达的(探测通过)
    /// 失败时返回人话错误(与 ensure_connected 一致)
    pub async fn get_live_host(&self, id: &str) -> Result<Arc<dyn Host>, String> {
        // 1. 先查缓存
        if let Some(_host) = self.get_host(id).await {
            // 2. 命中缓存 → 做廉价活性探测(带单飞)
            let lock = self.connect_lock_for(id).await;
            let _guard = lock.lock().await;

            // 二次检查:等锁期间可能已被别人刷新/驱逐
            if let Some(host2) = self.get_host(id).await {
                // 3. 执行 is_healthy(带短超时保护已在 RemoteLinuxHost 内实现)
                let start = std::time::Instant::now();
                let healthy = host2.is_healthy().await;
                let latency = start.elapsed().as_millis() as u64;

                if healthy {
                    // 探测成功 → 更新 health 成功时间 + 归零失败计数 + 发布恢复事件(若刚恢复)
                    self.update_health_success(id, latency).await;
                    return Ok(host2);
                } else {
                    // 探测失败 → 标记不健康,驱逐,发 lost 事件,然后走 ensure 重连
                    self.mark_unhealthy_internal(id, Some("活性探测失败".to_string())).await;
                    // 继续走到 ensure_connected 分支
                }
            }
        }

        // 缓存未命中或探测失败后已驱逐 → 走标准 ensure 路径(含单飞 + 冷却)
        self.ensure_connected(id).await
    }

    /// 强制刷新:无条件驱逐该 server 的缓存连接(若有),然后 ensure_connected
    /// 用于 Holder 明确知道当前 host 已死,或用户手动"重新测试连接"后的路径
    ///
    /// 刷新成功后会更新 health 并发布 Recovered 事件
    pub async fn refresh_host(&self, id: &str) -> Result<Arc<dyn Host>, String> {
        // 1. 驱逐旧缓存(若有)
        if self.hosts.write().await.remove(id).is_some() {
            info!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                "refresh_host 驱逐旧缓存连接"
            );
        }

        // 2. 走 ensure 建立新连接
        let host = self.ensure_connected(id).await?;

        // 3. 成功后补一个轻量健康标记(ensure 内部已更新 state,这里只补 health 成功时间)
        let now = chrono::Utc::now().to_rfc3339();
        self.update_health_fields(id, |h| {
            h.last_success_at = Some(now.clone());
            h.consecutive_failures = 0;
            // last_failure_* 保留上次失败信息,供前端诊断
        }).await;

        // 4. 发布恢复事件(刷新成功即视为一次恢复)
        self.publish_host_recovered(id, 0);

        Ok(host)
    }

    /// 显式标记该 server 的缓存连接不可用立即从 hosts 表移除,并把状态置 Disconnected
    /// Holder 在观测到可识别的 disconnect 错误后可调用,加速下一次访问触发重连幂等
    ///
    /// 内部会更新 health(递增连续失败计数 + 记失败原因/时间),并发布 HostConnectionLost 事件
    pub async fn mark_unhealthy(&self, id: &str) {
        self.mark_unhealthy_internal(id, None).await;
    }

    /// mark_unhealthy 的内部实现,reason 为 None 时使用默认文案
    async fn mark_unhealthy_internal(&self, id: &str, reason: Option<String>) {
        // 1. 驱逐缓存
        let removed = self.hosts.write().await.remove(id).is_some();

        // 2. 更新 ServerProfile.state
        self.update_state(id, ServerState::Disconnected).await;

        // 3. 更新 health(递增失败计数 + 记原因/时间)
        let reason_text = reason.clone().unwrap_or_else(|| "显式标记不健康".to_string());
        let now = chrono::Utc::now().to_rfc3339();
        let mut consecutive = 0u32;

        self.update_health_fields(id, |h| {
            h.last_failure_reason = Some(reason_text.clone());
            h.last_failure_at = Some(now.clone());
            h.consecutive_failures = h.consecutive_failures.saturating_add(1);
            consecutive = h.consecutive_failures;
        }).await;

        // 4. 发布 lost 事件(仅在确实发生驱逐或状态变更时发,避免重复刷)
        if removed {
            self.publish_host_lost(id, Some(reason_text), consecutive);
        }

        if removed {
            info!(
                target: "ncd_runtime::server_manager",
                server_id = %id,
                "mark_unhealthy 已驱逐缓存并标记 Disconnected"
            );
        }
    }

    /// 更新某 server 的 health 成功分支:记 last_success_at + 归零 consecutive_failures
    async fn update_health_success(&self, id: &str, _latency_ms: u64) {
        let now = chrono::Utc::now().to_rfc3339();
        self.update_health_fields(id, |h| {
            h.last_success_at = Some(now.clone());
            // 成功即归零连续失败
            if h.consecutive_failures > 0 {
                h.consecutive_failures = 0;
            }
            // 不清 last_failure_*,留作诊断
        }).await;
    }

    /// 通用 health 字段更新器:若 profile 不存在则静默跳过;若 health 为 None 则先初始化
    async fn update_health_fields<F>(&self, id: &str, mutator: F)
    where
        F: FnOnce(&mut ConnectionHealth),
    {
        let mut all = self.repo.load().await;
        if let Some(p) = all.iter_mut().find(|p| p.id == id) {
            let mut h = p.health.clone().unwrap_or_default();
            mutator(&mut h);
            p.health = Some(h);
            let _ = self.repo.save(&all).await;
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
                    .or_else(|| self.sync.credentials().get_password(&profile.id))
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
                    .or_else(|| self.sync.credentials().get_password(&profile.id));
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

    /// 注入事件总线(Tauri 侧 wiring 时调用)注入后 ServerManager 会在关键路径
    /// 发布 HostConnectionLost / HostConnectionRecovered 事件
    pub fn set_event_bus(&mut self, bus: Arc<dyn EventBus>) {
        self.event_sink = Some(bus);
    }

    /// 发布 HostConnectionLost 事件(若已注入 sink)
    fn publish_host_lost(&self, server_id: &str, reason: Option<String>, consecutive: u32) {
        if let Some(sink) = &self.event_sink {
            sink.publish(crate::events::DomainEvent::HostConnectionLost {
                server_id: server_id.to_string(),
                reason,
                consecutive_failures: consecutive,
            });
        }
    }

    /// 发布 HostConnectionRecovered 事件(若已注入 sink)
    fn publish_host_recovered(&self, server_id: &str, latency_ms: u64) {
        if let Some(sink) = &self.event_sink {
            sink.publish(crate::events::DomainEvent::HostConnectionRecovered {
                server_id: server_id.to_string(),
                latency_ms,
            });
        }
    }

    // ============================================================
    // P1 主动探活:后台低频健康 walker(用户可开关)
    // ============================================================

    /// 后台健康探活主循环
    ///
    /// 每轮读取 settings 的 remote_host_health_probe_enabled 和 remote_host_health_probe_interval_ms
    /// - enabled == false 时跳过本轮探测,仅 sleep interval 后继续
    /// - 只对落盘状态为 Connected 且当前 hosts 缓存命中的主机执行廉价 is_healthy
    /// - 探测失败时调用 mark_unhealthy_internal(会驱逐缓存,更新 state/health,发布 HostConnectionLost)
    ///
    /// 使用 MissedTickBehavior::Skip 避免堆积;支持通过 cancel 取消
    /// 由 Tauri 侧根据 AppSettings 条件 spawn / cancel + restart
    pub async fn run_health_probe_loop(
        &self,
        settings: Arc<RwLock<AppSettings>>,
        cancel: CancellationToken,
    ) {
        // 初始间隔(会被每轮动态读设置覆盖)
        let mut ticker = interval(std::time::Duration::from_millis(30_000));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(
                        target: "ncd_runtime::server_manager",
                        "health probe walker 收到取消信号，退出"
                    );
                    break;
                }
                _ = ticker.tick() => {
                    // 每轮动态读取设置,决定是否工作 + 当前间隔
                    let (enabled, interval_ms) = {
                        let cfg = settings.read().await;
                        (
                            cfg.remote_host_health_probe_enabled,
                            cfg.remote_host_health_probe_interval_ms,
                        )
                    };

                    if !enabled {
                        debug!(
                            target: "ncd_runtime::server_manager",
                            "health probe disabled by settings; skip this tick"
                        );
                        // 仍需尊重当前 interval(下轮再判断),避免 CPU 空转
                        ticker = interval(std::time::Duration::from_millis(interval_ms));
                        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                        continue;
                    }

                    // 应用当前 interval(若设置变化)
                    ticker = interval(std::time::Duration::from_millis(interval_ms));
                    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    // 枚举所有 profiles,筛选 state == Connected
                    let profiles = self.repo.load().await;
                    let connected_ids: Vec<String> = profiles
                        .into_iter()
                        .filter(|p| p.state == ServerState::Connected)
                        .map(|p| p.id)
                        .collect();

                    if connected_ids.is_empty() {
                        continue;
                    }

                    // 对每个 connected id:若缓存命中则做廉价 is_healthy;失败则 mark_unhealthy
                    for id in connected_ids {
                        // 快速检查缓存
                        let host_opt = self.hosts.read().await.get(&id).cloned();
                        let Some(host) = host_opt else {
                            // 缓存已无,说明已断开或被其他路径驱逐,跳过
                            continue;
                        };

                        let start = std::time::Instant::now();
                        let healthy = host.is_healthy().await;
                        let _latency = start.elapsed().as_millis() as u64;

                        if !healthy {
                            // 失败:走内部路径(会驱逐,更新 state/health,发 lost 事件)
                            self.mark_unhealthy_internal(&id, Some("后台探活失败".to_string()))
                                .await;
                        }
                    }
                }
            }
        }
    }
}

/// 给落盘的私钥文件设权限Unix 设 600(仅属主可读写,否则 ssh 会拒用);
/// Windows 上文件权限模型不同,依赖 NTFS ACL 继承用户目录权限,这里 no-op
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

/// 把连接错误分类成 (人话错误, 待确认 host key, 是否 mismatch)
/// host key 未知 / 不一致不是认证失败,要让前端走指纹确认 / 中间人告警分支,
/// 而不是当成普通"连接失败"红条
fn classify_connect_error(err: &HostError) -> (String, Option<HostKeyPrompt>, bool) {
    match err {
        HostError::HostKeyUnknown {
            host,
            port,
            key_kind,
            key_b64,
        } => (
            "首次连接该主机,请核对 host key 指纹后确认信任。".to_string(),
            Some(HostKeyPrompt {
                host: host.clone(),
                port: *port,
                key_kind: key_kind.clone(),
                key_b64: key_b64.clone(),
                fingerprint: ssh_key_fingerprint(key_b64),
            }),
            false,
        ),
        HostError::HostKeyMismatch {
            host,
            port,
            key_kind,
            key_b64,
        } => (
            "远端 host key 与已记录的不一致,疑似中间人攻击,连接已阻断。".to_string(),
            Some(HostKeyPrompt {
                host: host.clone(),
                port: *port,
                key_kind: key_kind.clone(),
                key_b64: key_b64.clone(),
                fingerprint: ssh_key_fingerprint(key_b64),
            }),
            true,
        ),
        other => (other.to_string(), None, false),
    }
}

/// 算 OpenSSH 风格公钥指纹 SHA256:<base64-no-pad(sha256(raw_key))>
/// 入参是 known_hosts 那段 base64 公钥;解码失败退回带原串的占位(仅展示用,不致命)
fn ssh_key_fingerprint(key_b64: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let raw = base64::engine::general_purpose::STANDARD
        .decode(key_b64)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(key_b64));
    let raw = match raw {
        Ok(bytes) => bytes,
        Err(_) => return format!("SHA256:(无法解析) {key_b64}"),
    };
    let digest = Sha256::digest(&raw);
    let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest);
    format!("SHA256:{encoded}")
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
            health: None,
            webui_url: None,
        }
    }

    fn make_mgr(root: &Path) -> (ServerManager, Arc<InMemoryCredentialStore>) {
        let creds = Arc::new(InMemoryCredentialStore::default());
        let mgr = ServerManager::new(root, creds.clone());
        (mgr, creds)
    }

    #[cfg(test)]
    impl ServerManager {
        async fn test_has_connect_lock(&self, id: &str) -> bool {
            self.connect_locks.read().await.contains_key(id)
        }

        async fn test_has_auto_connect_cooldown(&self, id: &str) -> bool {
            self.auto_connect_cooldown_until.read().await.contains_key(id)
        }

        async fn test_seed_auto_connect_cooldown(&self, id: &str) {
            self.auto_connect_cooldown_until.write().await.insert(
                id.to_string(),
                std::time::Instant::now() + std::time::Duration::from_secs(300),
            );
        }

        async fn test_touch_connect_lock(&self, id: &str) {
            let _ = self.connect_lock_for(id).await;
        }

        async fn test_cooldown_map_len(&self) -> usize {
            self.auto_connect_cooldown_until.read().await.len()
        }

        async fn test_set_cooldown_until(&self, id: &str, until: std::time::Instant) {
            self.auto_connect_cooldown_until
                .write()
                .await
                .insert(id.to_string(), until);
        }

        async fn test_prune_expired_cooldowns(&self) {
            self.prune_expired_auto_connect_cooldowns().await;
        }
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
    async fn delete_server_clears_connect_locks_and_cooldown_maps() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());

        mgr.add_server(make_profile("s1", "A"), None).await.unwrap();
        mgr.test_touch_connect_lock("s1").await;
        mgr.test_seed_auto_connect_cooldown("s1").await;
        assert!(mgr.test_has_connect_lock("s1").await);
        assert!(mgr.test_has_auto_connect_cooldown("s1").await);

        mgr.delete_server("s1").await.unwrap();
        assert!(!mgr.test_has_connect_lock("s1").await);
        assert!(!mgr.test_has_auto_connect_cooldown("s1").await);
    }

    #[tokio::test]
    async fn prune_expired_auto_connect_cooldowns_drops_stale_entries() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());

        mgr.test_set_cooldown_until(
            "expired",
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        )
        .await;
        mgr.test_set_cooldown_until(
            "fresh",
            std::time::Instant::now() + std::time::Duration::from_secs(60),
        )
        .await;
        assert_eq!(mgr.test_cooldown_map_len().await, 2);

        mgr.test_prune_expired_cooldowns().await;
        assert_eq!(mgr.test_cooldown_map_len().await, 1);
        assert!(mgr.test_has_auto_connect_cooldown("fresh").await);
        assert!(!mgr.test_has_auto_connect_cooldown("expired").await);
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
            host_key_prompt: None,
            host_key_mismatch: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("latencyMs"));
        assert!(json.contains("osInfo"));
        // 无 host key 待确认时 prompt 字段应被 skip,不污染常规报告
        assert!(!json.contains("hostKeyPrompt"));
    }

    #[test]
    fn production_host_key_policy_is_tofu_not_insecure() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());
        match mgr.host_key_policy() {
            HostKeyPolicy::AcceptOnFirstUse { known_hosts_path } => {
                assert!(known_hosts_path.ends_with("secrets/known_hosts"));
            }
            other => panic!("生产 host key 策略不应是 {other:?},必须是 AcceptOnFirstUse TOFU"),
        }
    }

    #[tokio::test]
    async fn confirm_host_key_appends_unknown_then_matches() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());
        mgr.add_server(make_profile("s1", "A"), None).await.unwrap();

        // 首次确认:未知 -> 追加到 known_hosts
        mgr.confirm_host_key("s1", "ssh-ed25519", "AAAAkeyfirst")
            .await
            .unwrap();
        let known = tokio::fs::read_to_string(root.path().join("secrets").join("known_hosts"))
            .await
            .unwrap();
        assert!(known.contains("192.168.1.100 ssh-ed25519 AAAAkeyfirst"));

        // 再确认同一把 key:已 Match,幂等成功
        mgr.confirm_host_key("s1", "ssh-ed25519", "AAAAkeyfirst")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn confirm_host_key_rejects_mismatch_without_overwrite() {
        let root = tempdir().unwrap();
        let (mgr, _) = make_mgr(root.path());
        mgr.add_server(make_profile("s1", "A"), None).await.unwrap();

        mgr.confirm_host_key("s1", "ssh-ed25519", "AAAAoriginal")
            .await
            .unwrap();

        // 同主机但换了 key:疑似中间人,必须拒绝,且不得覆盖原条目
        let err = mgr
            .confirm_host_key("s1", "ssh-ed25519", "AAAAattacker")
            .await
            .unwrap_err();
        assert!(err.contains("中间人") || err.contains("不同"));

        let known = tokio::fs::read_to_string(root.path().join("secrets").join("known_hosts"))
            .await
            .unwrap();
        assert!(known.contains("AAAAoriginal"));
        assert!(!known.contains("AAAAattacker"));
    }

    #[test]
    fn ssh_key_fingerprint_is_sha256_prefixed() {
        // base64("hi") = "aGk=";算得出固定的 SHA256 指纹格式
        let fp = ssh_key_fingerprint("aGk=");
        assert!(fp.starts_with("SHA256:"));
        assert!(!fp.contains('='), "OpenSSH 指纹不带 base64 padding");
    }
}
