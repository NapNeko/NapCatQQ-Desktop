//! 应用自带的 WebUI：登录用户名、监听口、登录密钥和账号；运行期接口要的口和密钥在这里缓存

use super::*;

/// 运行期接口每次都要的 WebUI 口和登录密钥。远端读一遍配置要十几次 SFTP 往返，
/// 麦麦的表情包网格、导入进度轮询一秒好几个请求，每次现读页面就卡住了
#[derive(Debug, Clone)]
pub(super) struct WebUiEndpoint {
    pub(super) listen_port: u16,
    pub(super) auth_key: String,
    /// 远端主机相对 UTC 的秒数，本机为 None（按桌面端时区）
    pub(super) utc_offset_secs: Option<i32>,
    read_at: Instant,
}

/// 桌面端经手的改动当场作废；这个时限兜的是用户在应用自己的 WebUI 或盘上改了口令
const WEBUI_ENDPOINT_TTL: Duration = Duration::from_secs(300);

fn webui_tunnel_key(instance: &AppInstance) -> String {
    format!("{}:webui", instance.id.as_str())
}

impl AppManager {
    pub fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        adapter.integration().webui_url(instance, public_host)
    }

    pub(super) async fn webui_login_username(&self, instance: &AppInstance) -> String {
        if let Some(account) = self.webui_account(instance).await {
            let name = account.username.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
        self.remembered_secret(instance, SECRET_WEBUI_USERNAME)
            .unwrap_or_else(|| "astrbot".into())
    }

    /// 桌面端打开 WebUI 用的本机口：本机即实例口，远端是 SSH `-L` 分配口。
    pub async fn desktop_loopback_port(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        self.ensure_local_to_remote_tunnel(instance).await
    }

    /// WebUI 在应用机上的 HTTP 口（AstrBot 是 dashboard.port，不是 OneBot 口）。
    pub async fn webui_listen_port(&self, instance: &AppInstance) -> u16 {
        self.webui_endpoint(instance).await.listen_port
    }

    /// 口和密钥一次读齐记下来。读失败不记：给个兜底口，下次再读
    pub(super) async fn webui_endpoint(&self, instance: &AppInstance) -> WebUiEndpoint {
        if let Some(hit) = self
            .webui_endpoints
            .lock()
            .ok()
            .and_then(|map| map.get(&instance.id).cloned())
            .filter(|e| e.read_at.elapsed() < WEBUI_ENDPOINT_TTL)
        {
            return hit;
        }
        match self.read_config(&instance.id).await {
            Ok(env) => {
                let utc_offset_secs = match server_id_of_host(&instance.host_id) {
                    Some(_) => match self.resolve_host(&instance.host_id).await {
                        Ok(host) => remote_utc_offset(host.as_ref()).await,
                        Err(_) => None,
                    },
                    None => None,
                };
                let endpoint = WebUiEndpoint {
                    listen_port: env.config.webui_port().unwrap_or(instance.port),
                    auth_key: env.config.webui_auth_key().to_string(),
                    utc_offset_secs,
                    read_at: Instant::now(),
                };
                if let Ok(mut map) = self.webui_endpoints.lock() {
                    map.insert(instance.id.clone(), endpoint.clone());
                }
                endpoint
            }
            Err(_) => WebUiEndpoint {
                listen_port: self
                    .registry
                    .get(&instance.framework_id)
                    .map(|adapter| adapter.webui_fallback_port(instance))
                    .unwrap_or(instance.port),
                auth_key: String::new(),
                utc_offset_secs: None,
                read_at: Instant::now(),
            },
        }
    }

    pub(super) fn forget_webui_endpoint(&self, id: &AppInstanceId) {
        if let Ok(mut map) = self.webui_endpoints.lock() {
            map.remove(id);
        }
    }

    /// 打开 WebUI：远端隧道打到 WebUI 口，再交给 integration 拼 URL。
    pub async fn desktop_webui_loopback_port(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        let remote = self.webui_listen_port(instance).await;
        self.ensure_local_to_remote_port_tunnel(instance, remote, webui_tunnel_key(instance))
            .await
    }

    /// 读盘拿到的 WebUI 登录密钥；配置读失败或没有密钥时返回空串。
    pub async fn webui_auth_key(&self, id: &AppInstanceId) -> String {
        match self.store.require(id).await {
            Ok(instance) => self.webui_endpoint(&instance).await.auth_key,
            Err(_) => String::new(),
        }
    }

    /// 用户名密码类 WebUI 的账号：用户名以落盘为准，密码只有桌面端自己设过才有。
    /// 落盘读不到时也给出默认用户名，让用户至少知道该填什么。
    /// 关键：password 字段仅在桌面端有明文密码时返回；若落盘是哈希且无明文，返回 None 引导用户手动登录或重置。
    pub async fn webui_account(&self, instance: &AppInstance) -> Option<AppWebUiAccount> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        if adapter.manifest().webui_auth != AppWebUiAuthKind::UserPassword {
            return None;
        }
        let remembered = self.remembered_secret(instance, SECRET_WEBUI_PASSWORD);
        let probe = match self.resolve_host(&instance.host_id).await {
            Ok(host) => adapter
                .read_webui_account(host.as_ref(), instance, remembered.as_deref())
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        let can_reset = !matches!(instance.state, AppInstanceState::Running);

        // 如果有明文密码（桌面端设置过），直接用明文
        if let Some(ref pwd) = remembered
            && !pwd.is_empty()
        {
            return Some(match probe {
                Some(p) => AppWebUiAccount {
                    username: p.username,
                    password: Some(pwd.clone()),
                    password_matches: p.password_matches,
                    can_reset,
                },
                None => AppWebUiAccount {
                    username: self
                        .remembered_secret(instance, SECRET_WEBUI_USERNAME)
                        .unwrap_or_default(),
                    password: Some(pwd.clone()),
                    password_matches: None,
                    can_reset,
                },
            });
        }

        // 无明文密码：检查落盘是否为哈希
        let stored_hash_is_hash = match &probe {
            Some(p) => p.stored_is_hash,
            None => false,
        };

        // 无明文且落盘是哈希 -> 不返回 password，password_matches = None，引导用户去 WebUI 登录或重置
        if stored_hash_is_hash {
            return Some(match probe {
                Some(p) => AppWebUiAccount {
                    username: p.username,
                    password: None,
                    password_matches: None,
                    can_reset,
                },
                None => AppWebUiAccount {
                    username: self
                        .remembered_secret(instance, SECRET_WEBUI_USERNAME)
                        .unwrap_or_default(),
                    password: None,
                    password_matches: None,
                    can_reset,
                },
            });
        }

        // 无明文且落盘无哈希（首启状态）
        Some(match probe {
            Some(p) => AppWebUiAccount {
                username: p.username,
                password: None,
                password_matches: None,
                can_reset,
            },
            None => AppWebUiAccount {
                username: self
                    .remembered_secret(instance, SECRET_WEBUI_USERNAME)
                    .unwrap_or_default(),
                password: None,
                password_matches: None,
                can_reset,
            },
        })
    }

    /// 重置 WebUI 密码（None = 随机生成）：实例必须已停止，写完下次启动生效。
    pub async fn reset_webui_password(
        &self,
        id: &AppInstanceId,
        password: Option<String>,
    ) -> Result<AppWebUiAccount, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.manifest().webui_auth != AppWebUiAuthKind::UserPassword {
            return Err(AppFrameworkError::ConfigUnsupported(
                instance.framework_id.as_str().to_string(),
            ));
        }
        if matches!(instance.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::Validation(
                "实例运行中，先停止再重置密码".to_string(),
            ));
        }
        let password = resolve_webui_password(adapter.as_ref(), password)?;
        let host = self.resolve_host(&instance.host_id).await?;
        let written = adapter
            .write_webui_password(host.as_ref(), &instance, &password)
            .await;
        self.forget_webui_endpoint(id);
        written?;
        self.remember_secret(id, SECRET_WEBUI_PASSWORD, &password);
        Ok(self
            .webui_account(&instance)
            .await
            .unwrap_or(AppWebUiAccount {
                username: String::new(),
                password: Some(password),
                password_matches: Some(true),
                can_reset: true,
            }))
    }
}
