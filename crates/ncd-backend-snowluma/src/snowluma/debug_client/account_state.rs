//! 区分 QQ 未登录与 OneBot 账号初始化未完成。

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{Duration, Method, RequestKind, SnowLumaDebugClient, SnowLumaDebugError, read_text};

const DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
struct Connections {
    list: Vec<AccountConnection>,
}

#[derive(Deserialize)]
struct AccountConnection {
    uin: String,
    #[serde(default, rename = "databaseMigration")]
    database_migration: Option<DatabaseMigration>,
}

#[derive(Deserialize)]
struct DatabaseMigration {
    phase: String,
    #[serde(default)]
    usable: bool,
}

#[derive(Deserialize)]
struct Processes {
    list: Vec<AccountProcess>,
}

#[derive(Deserialize)]
struct AccountProcess {
    #[serde(default)]
    uin: String,
    #[serde(default)]
    status: String,
    #[serde(default, rename = "loggedIn")]
    logged_in: bool,
}

impl SnowLumaDebugClient {
    pub(super) async fn account_unavailable(
        &self,
        uin: &str,
        budget: Duration,
    ) -> SnowLumaDebugError {
        // 诊断不能延长动作时限，也不能让旧版本缺少诊断接口时掩盖原始错误。
        let reason = tokio::time::timeout(
            budget.min(DIAGNOSTIC_TIMEOUT),
            self.account_unavailable_reason(uin),
        )
        .await
        .ok()
        .flatten();
        match reason {
            Some(reason) => SnowLumaDebugError::SessionNotReady(reason.into()),
            None => SnowLumaDebugError::NotOnline,
        }
    }

    async fn account_unavailable_reason(&self, uin: &str) -> Option<&'static str> {
        if let Some(connections) = self
            .diagnostic_json::<Connections>("/api/connections")
            .await
            && let Some(account) = connections.list.iter().find(|account| account.uin == uin)
        {
            // 上游错误可能含完整配置地址，仅投影阶段，不把凭据拼进提示。
            return Some(match &account.database_migration {
                Some(migration) if migration.phase == "failed" => {
                    "账号配置或数据库初始化失败，请检查 SnowLuma 控制台日志"
                }
                Some(migration) if !migration.usable && migration.phase == "preparing" => {
                    "账号数据库仍在准备，请稍后重试"
                }
                Some(migration) if !migration.usable && migration.phase == "migrating" => {
                    "账号数据库尚未就绪，请稍后重试"
                }
                _ => "账号会话正在建立或切换，请稍后重试",
            });
        }
        let processes = self.diagnostic_json::<Processes>("/api/processes").await?;
        processes
            .list
            .iter()
            .any(|process| process.uin == uin && (process.status == "online" || process.logged_in))
            .then_some("QQ 已登录，但 OneBot 会话尚未建立，请检查 SnowLuma 账号初始化日志")
    }

    async fn diagnostic_json<T: DeserializeOwned>(&self, path: &str) -> Option<T> {
        let response = self
            .authed(
                Method::GET,
                path,
                None,
                RequestKind::Timed(DIAGNOSTIC_TIMEOUT),
            )
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        serde_json::from_str(&read_text(response).await.ok()?).ok()
    }
}
