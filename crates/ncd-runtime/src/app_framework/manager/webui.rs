//! 应用 WebUI 的浏览器入口、SSH 隧道与登录信息。

use super::*;

/// 同一次打开共用端口与密钥，避免远端重复读取。
#[derive(Debug, Clone)]
pub(super) struct WebUiEndpoint {
    pub(super) listen_port: u16,
    pub(super) auth_key: String,
    read_at: Instant,
}

/// 桌面端经手的改动当场作废；这个时限兜的是用户在应用自己的 WebUI 或盘上改了口令
const WEBUI_ENDPOINT_TTL: Duration = Duration::from_secs(300);

fn webui_tunnel_key(instance: &AppInstance) -> String {
    format!("{}:webui", instance.id.as_str())
}

impl AppManager {
    pub(super) async fn prime_live_port(
        &self,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        if !adapter.wants_live_port() || instance.state != AppInstanceState::Running {
            return Ok(());
        }
        self.forget_webui_endpoint(&instance.id);
        let port = self.desktop_webui_loopback_port(instance).await?;
        adapter.note_live_port(instance.id.as_str(), port);
        Ok(())
    }

    pub fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        adapter.integration().webui_url(instance, public_host)
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
        let result = async {
            let adapter = self.registry.get(&instance.framework_id)?;
            let host = self.resolve_host(&instance.host_id).await?;
            adapter.read_webui_endpoint(host.as_ref(), instance).await
        }
        .await;
        match result {
            Ok((listen_port, auth_key)) => {
                let endpoint = WebUiEndpoint {
                    listen_port,
                    auth_key,
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

    /// 桌面端要打开的 WebUI：地址一律是本机回环（远端经 SSH 隧道），`path` 接在后面进到指定页；
    /// 连同登录密钥和账号一起给，前端好代填
    pub async fn open_webui(
        &self,
        id: &AppInstanceId,
        path: Option<&str>,
    ) -> Result<AppInstanceWebUi, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        // 用户在框架 WebUI 中修改端口或密钥后，下次打开立即使用新值。
        self.forget_webui_endpoint(id);
        let port = self.desktop_webui_loopback_port(&instance).await?;
        let mut view = instance.clone();
        view.port = port;
        let base = self
            .webui_url(&view, "127.0.0.1")
            .ok_or_else(|| AppFrameworkError::Validation("该应用端没有 WebUI".into()))?;
        Ok(AppInstanceWebUi {
            url: join_webui_url(&base, path),
            auth_key: self.webui_auth_key(&instance.id).await,
            account: self.webui_account(&instance).await,
        })
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

        // 明文只有桌面端自己设过才有；没有明文（首启，或落盘只剩哈希）就不给密码、也不判对不对，
        // 引导用户去 WebUI 登录或重置
        let password = remembered.filter(|p| !p.is_empty());
        Some(match probe {
            Some(p) => AppWebUiAccount {
                username: p.username,
                password_matches: password.as_ref().and(p.password_matches),
                password,
                can_reset,
            },
            None => AppWebUiAccount {
                username: self.fallback_webui_username(instance),
                password,
                password_matches: None,
                can_reset,
            },
        })
    }

    /// 落盘读不到时的用户名：桌面端记下的那个，没有就用框架首启时的默认名
    fn fallback_webui_username(&self, instance: &AppInstance) -> String {
        self.remembered_secret(instance, SECRET_WEBUI_USERNAME)
            .unwrap_or_else(|| {
                self.registry
                    .get(&instance.framework_id)
                    .map(|adapter| adapter.default_webui_username().to_string())
                    .unwrap_or_default()
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
