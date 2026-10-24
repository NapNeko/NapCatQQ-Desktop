// 单 Bot 客户端在已验证的宿主机 PID 与 WebUI PID 命名空间之间转换。
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::snowluma::error::SnowLumaWebUiError;
use crate::snowluma::webui_client::{
    AgreementsPayload, AuthState, HookProcessInfo, OneBotInstanceInfo, QqPortLoginInfo,
    SnowLumaWebUiClient,
};

pub(super) struct MappedSnowLumaWebUiClient {
    inner: Arc<dyn SnowLumaWebUiClient>,
    host_pid: u32,
    webui_pid: u32,
}

impl MappedSnowLumaWebUiClient {
    /// The caller must verify both PIDs identify the same live QQ process.
    pub(super) fn new(
        inner: Arc<dyn SnowLumaWebUiClient>,
        host_pid: u32,
        webui_pid: u32,
    ) -> Result<Self, SnowLumaWebUiError> {
        if host_pid == 0 || webui_pid == 0 {
            return Err(Self::identity_error());
        }
        Ok(Self {
            inner,
            host_pid,
            webui_pid,
        })
    }

    fn identity_error() -> SnowLumaWebUiError {
        SnowLumaWebUiError::ServerRejected {
            endpoint: "pid-map".into(),
            message: "QQ 进程与已验证的 WebUI PID 映射不符".into(),
        }
    }

    fn to_webui(&self, host_pid: u32) -> Result<u32, SnowLumaWebUiError> {
        if host_pid != self.host_pid {
            return Err(Self::identity_error());
        }
        Ok(self.webui_pid)
    }

    fn to_host(&self, mut process: HookProcessInfo) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        if process.pid != self.webui_pid {
            return Err(Self::identity_error());
        }
        process.pid = self.host_pid;
        Ok(process)
    }
}

#[async_trait]
impl SnowLumaWebUiClient for MappedSnowLumaWebUiClient {
    async fn wait_ready(
        &self,
        timeout: Duration,
        dead_check: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError> {
        self.inner.wait_ready(timeout, dead_check).await
    }

    async fn login(&self) -> Result<(), SnowLumaWebUiError> {
        self.inner.login().await
    }

    async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
        self.inner.logout().await
    }

    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        // Never expose an unrelated API PID that happens to equal the host PID.
        let mut target = self
            .inner
            .list_processes()
            .await?
            .into_iter()
            .filter(|process| process.pid == self.webui_pid);
        let Some(process) = target.next() else {
            return Ok(Vec::new());
        };
        if target.next().is_some() {
            return Err(Self::identity_error());
        }
        Ok(vec![self.to_host(process)?])
    }

    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        self.inner.list_qq_instances().await
    }

    async fn probe_process_login_info(
        &self,
        pid: u32,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
        self.inner
            .probe_process_login_info(self.to_webui(pid)?)
            .await
    }

    async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        self.to_host(self.inner.load_process(self.to_webui(pid)?).await?)
    }

    async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        self.to_host(self.inner.unload_process(self.to_webui(pid)?).await?)
    }

    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
        self.inner.get_auth_state().await
    }

    async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError> {
        self.inner.get_agreements().await
    }

    async fn record_agreement_consent(&self, version: &str) -> Result<(), SnowLumaWebUiError> {
        self.inner.record_agreement_consent(version).await
    }

    async fn update_onebot_config(
        &self,
        uin: &str,
        config: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError> {
        self.inner.update_onebot_config(uin, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use crate::snowluma::webui_client::HookProcessStatus;

    const HOST_PID: u32 = 430336;
    const WEBUI_PID: u32 = 46;

    fn process(pid: u32) -> HookProcessInfo {
        HookProcessInfo {
            pid,
            name: "qq".into(),
            path: "/opt/QQ/qq".into(),
            uin: "10001".into(),
            status: HookProcessStatus::Online,
            logged_in: Some(true),
            connected: Some(true),
            injected: Some(true),
            error: String::new(),
        }
    }

    struct Client {
        processes: Mutex<Vec<HookProcessInfo>>,
        calls: Mutex<Vec<String>>,
        response_pid: AtomicU32,
        fail: AtomicBool,
    }

    impl Client {
        fn record(&self, call: impl Into<String>) -> Result<(), SnowLumaWebUiError> {
            self.calls.lock().unwrap().push(call.into());
            if self.fail.load(Ordering::SeqCst) {
                return Err(SnowLumaWebUiError::Status {
                    endpoint: "/fixture".into(),
                    status: 503,
                    message: "service unavailable".into(),
                });
            }
            Ok(())
        }
    }

    #[async_trait]
    impl SnowLumaWebUiClient for Client {
        async fn wait_ready(
            &self,
            timeout: Duration,
            dead_check: Box<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<(), SnowLumaWebUiError> {
            self.record(format!("ready:{}:{}", timeout.as_millis(), dead_check()))
        }
        async fn login(&self) -> Result<(), SnowLumaWebUiError> {
            self.record("login")
        }
        async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
            self.record("logout")
        }
        async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
            self.record("processes")?;
            Ok(self.processes.lock().unwrap().clone())
        }
        async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
            self.record("qq")?;
            Ok(vec![OneBotInstanceInfo {
                uin: "10001".into(),
                nickname: "fixture".into(),
            }])
        }
        async fn probe_process_login_info(
            &self,
            pid: u32,
        ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
            self.record(format!("probe:{pid}"))?;
            Ok(Some(QqPortLoginInfo {
                port: 10080,
                uin: "10001".into(),
                uid: Some("u_fixture".into()),
                nickname: Some("fixture".into()),
                logged_in: true,
            }))
        }
        async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            self.record(format!("load:{pid}"))?;
            Ok(process(self.response_pid.load(Ordering::SeqCst)))
        }
        async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            self.record(format!("unload:{pid}"))?;
            Ok(process(self.response_pid.load(Ordering::SeqCst)))
        }
        async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
            self.record("auth")?;
            Ok(AuthState {
                must_change_password: true,
            })
        }
        async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError> {
            self.record("agreements")?;
            Ok(AgreementsPayload {
                version: "fixture-version".into(),
                consent_required: true,
                documents: Vec::new(),
            })
        }
        async fn record_agreement_consent(&self, version: &str) -> Result<(), SnowLumaWebUiError> {
            self.record(format!("consent:{version}"))
        }
        async fn update_onebot_config(
            &self,
            uin: &str,
            config: &serde_json::Value,
        ) -> Result<bool, SnowLumaWebUiError> {
            self.record(format!("config:{uin}:{config}"))?;
            Ok(true)
        }
    }

    fn fixture() -> (Arc<Client>, MappedSnowLumaWebUiClient) {
        let inner = Arc::new(Client {
            processes: Mutex::new(vec![process(WEBUI_PID), process(HOST_PID), process(19)]),
            calls: Mutex::new(Vec::new()),
            response_pid: AtomicU32::new(WEBUI_PID),
            fail: AtomicBool::new(false),
        });
        let mapped = MappedSnowLumaWebUiClient::new(inner.clone(), HOST_PID, WEBUI_PID).unwrap();
        (inner, mapped)
    }

    #[tokio::test]
    async fn list_maps_only_the_verified_target_and_preserves_its_state() {
        let (_, client) = fixture();
        assert_eq!(
            client.list_processes().await.unwrap(),
            vec![process(HOST_PID)]
        );
    }

    #[tokio::test]
    async fn api_pid_equal_to_host_pid_cannot_impersonate_a_missing_target() {
        let (inner, client) = fixture();
        *inner.processes.lock().unwrap() = vec![process(HOST_PID), process(19)];
        assert!(client.list_processes().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn actions_use_api_pid_and_return_host_pid_without_double_mapping() {
        let (inner, client) = fixture();
        assert_eq!(
            client.load_process(HOST_PID).await.unwrap(),
            process(HOST_PID)
        );
        assert_eq!(
            client.unload_process(HOST_PID).await.unwrap(),
            process(HOST_PID)
        );
        let info = client
            .probe_process_login_info(HOST_PID)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(info.uin, "10001");
        assert!(info.logged_in);
        assert_eq!(
            *inner.calls.lock().unwrap(),
            ["load:46", "unload:46", "probe:46"]
        );
    }

    #[tokio::test]
    async fn non_target_host_pids_are_rejected_before_any_http_action() {
        let (inner, client) = fixture();
        for pid in [0, WEBUI_PID, 19, HOST_PID + 1] {
            assert!(client.load_process(pid).await.is_err());
            assert!(client.unload_process(pid).await.is_err());
            assert!(client.probe_process_login_info(pid).await.is_err());
        }
        assert!(inner.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unexpected_action_response_and_duplicate_target_rows_are_rejected() {
        let (inner, client) = fixture();
        inner.response_pid.store(HOST_PID, Ordering::SeqCst);
        assert!(client.load_process(HOST_PID).await.is_err());
        assert!(client.unload_process(HOST_PID).await.is_err());
        *inner.processes.lock().unwrap() = vec![process(WEBUI_PID), process(WEBUI_PID)];
        assert!(client.list_processes().await.is_err());
    }

    fn assert_original_error(error: SnowLumaWebUiError) {
        assert!(
            matches!(error, SnowLumaWebUiError::Status { endpoint, status: 503, message }
            if endpoint == "/fixture" && message == "service unavailable")
        );
    }

    #[tokio::test]
    async fn underlying_errors_keep_the_original_type_status_and_endpoint() {
        let (inner, client) = fixture();
        inner.fail.store(true, Ordering::SeqCst);
        assert_original_error(client.list_processes().await.unwrap_err());
        assert_original_error(client.load_process(HOST_PID).await.unwrap_err());
        assert_original_error(client.unload_process(HOST_PID).await.unwrap_err());
        assert_original_error(client.probe_process_login_info(HOST_PID).await.unwrap_err());
        assert_original_error(client.login().await.unwrap_err());
    }

    #[tokio::test]
    async fn pid_free_operations_and_the_readiness_callback_are_delegated_unchanged() {
        let (inner, client) = fixture();
        client
            .wait_ready(Duration::from_millis(1234), Box::new(|| true))
            .await
            .unwrap();
        client.login().await.unwrap();
        client.logout().await.unwrap();
        assert!(client.get_auth_state().await.unwrap().must_change_password);
        assert_eq!(
            client.get_agreements().await.unwrap().version,
            "fixture-version"
        );
        client
            .record_agreement_consent("accepted-version")
            .await
            .unwrap();
        assert_eq!(
            client.list_qq_instances().await.unwrap(),
            vec![OneBotInstanceInfo {
                uin: "10001".into(),
                nickname: "fixture".into(),
            }]
        );
        let config = serde_json::json!({"enabled": true});
        assert!(client.update_onebot_config("10001", &config).await.unwrap());
        assert_eq!(
            *inner.calls.lock().unwrap(),
            [
                "ready:1234:true",
                "login",
                "logout",
                "auth",
                "agreements",
                "consent:accepted-version",
                "qq",
                "config:10001:{\"enabled\":true}",
            ]
        );
    }

    #[test]
    fn zero_pids_are_not_a_valid_mapping() {
        let (inner, _) = fixture();
        assert!(MappedSnowLumaWebUiClient::new(inner.clone(), 0, WEBUI_PID).is_err());
        assert!(MappedSnowLumaWebUiClient::new(inner, HOST_PID, 0).is_err());
    }
}
