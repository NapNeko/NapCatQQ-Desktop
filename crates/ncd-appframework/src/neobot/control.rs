//! NeoBot 面板控制口：请它优雅关闭。
//!
//! 为什么要走 HTTP 而不是发信号：Windows 上 `loop.add_signal_handler` 在 Proactor
//! 下不可用，NeoBot 只接得到 Ctrl+C；而桌面端 spawn 到的是**无控制台**的子进程，
//! `GenerateConsoleCtrlEvent` 也打不到。桌面端唯一能用的「请它退」手段就是它自己的
//! 面板接口（NeoBot 1.2.3 起提供 `POST /api/admin/shutdown`）。
//!
//! 鉴权：本机来源 + 面板密码。先直接请求（面板未设密码时本机即可），
//! 被拒时才用记住的密码登录换 token 再试一次——这样「设过密码」和「没设密码」
//! 两种实例都能关。

use std::time::Duration;

use ncd_host::{Host, HostPath};
use serde::Deserialize;
use serde_json::json;

/// 需要 NeoBot 至少这个版本才有 `/api/admin/shutdown`。
/// 桌面端据此决定要不要走优雅关闭；低版本只能直接收树。
pub const MIN_VERSION_WITH_SHUTDOWN_ENDPOINT: &str = "1.2.3";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_PATH: &str = "/api/admin/shutdown";
const LOGIN_PATH: &str = "/api/auth/login";

#[derive(Debug, Deserialize)]
struct LoginResponse {
    #[serde(default)]
    token: String,
    #[serde(default)]
    csrf_token: String,
}

/// 请求结果。调用方据此决定「继续等进程退出」还是「直接收树」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownRequest {
    /// 端点接受了请求（200）。**不等于已关闭**——调用方仍须轮询进程存活。
    Accepted,
    /// 端点不存在（404）：实例版本低于 `MIN_VERSION_WITH_SHUTDOWN_ENDPOINT`
    EndpointMissing,
    /// 本机也没拿到授权（面板设了密码而桌面端不知道）
    Unauthorized,
    /// 面板打不通：没在跑、口不对、或端口占用
    Unreachable(String),
    /// 其它非成功响应
    Failed(String),
}

/// 面板控制口地址。**只连回环**：这是关停整个进程的入口。
pub fn shutdown_url(dashboard_port: u16) -> String {
    format!("http://127.0.0.1:{dashboard_port}{SHUTDOWN_PATH}")
}

fn login_url(dashboard_port: u16) -> String {
    format!("http://127.0.0.1:{dashboard_port}{LOGIN_PATH}")
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .pool_idle_timeout(Duration::from_secs(30))
        // 本机回环不该走代理；用户环境里的 HTTP_PROXY 会把 127.0.0.1 也代理走
        .no_proxy()
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))
}

/// 用面板密码换 token + CSRF。密码不可用（未设 / 记错）时返回 None。
async fn login(client: &reqwest::Client, dashboard_port: u16, password: &str) -> Option<LoginResponse> {
    let resp = client
        .post(login_url(dashboard_port))
        .json(&json!({ "password": password }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: LoginResponse = resp.json().await.ok()?;
    (!parsed.token.is_empty()).then_some(parsed)
}

/// 请 NeoBot 优雅关闭。
///
/// `password` 是桌面端记住的面板密码（可能为 None：面板从没设过密码）。
/// 返回 `Accepted` 只代表端点接受了；真正退出由调用方轮询确认。
pub async fn request_graceful_shutdown(
    dashboard_port: u16,
    password: Option<&str>,
) -> ShutdownRequest {
    if dashboard_port == 0 {
        return ShutdownRequest::Unreachable("实例没有可用面板端口".to_string());
    }
    let client = match client() {
        Ok(c) => c,
        Err(e) => return ShutdownRequest::Unreachable(e),
    };

    // 先不带凭据试一次：面板没设密码时本机来源就直接放行
    let first = match client.post(shutdown_url(dashboard_port)).send().await {
        Ok(r) => r,
        Err(e) => return ShutdownRequest::Unreachable(e.to_string()),
    };
    if first.status().is_success() {
        return ShutdownRequest::Accepted;
    }
    if first.status().as_u16() == 404 {
        // 端点不存在 = 版本太老，再带凭据试也没用
        return ShutdownRequest::EndpointMissing;
    }
    if first.status().as_u16() != 401 && first.status().as_u16() != 403 {
        return ShutdownRequest::Failed(format!("面板返回 {}", first.status()));
    }

    // 401/403 有两种可能：面板设了密码，或远端管理被关。
    // 前者能用密码解决，后者不行——都试一次密码登录，失败就如实报未授权。
    let Some(password) = password.filter(|p| !p.is_empty()) else {
        return ShutdownRequest::Unauthorized;
    };
    let Some(session) = login(&client, dashboard_port, password).await else {
        return ShutdownRequest::Unauthorized;
    };
    let second = match client
        .post(shutdown_url(dashboard_port))
        .header("X-Token", &session.token)
        .header("X-CSRF-Token", &session.csrf_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return ShutdownRequest::Unreachable(e.to_string()),
    };
    if second.status().is_success() {
        return ShutdownRequest::Accepted;
    }
    if second.status().as_u16() == 404 {
        return ShutdownRequest::EndpointMissing;
    }
    if second.status().as_u16() == 401 || second.status().as_u16() == 403 {
        return ShutdownRequest::Unauthorized;
    }
    ShutdownRequest::Failed(format!("面板返回 {}", second.status()))
}

/// 面板配置里的端口；读不到就用调用方给的回落口。
pub async fn dashboard_port(
    host: &dyn Host,
    install_dir: &HostPath,
    fallback: u16,
) -> u16 {
    match crate::neobot::config::read_neobot_config(host, install_dir).await {
        Ok((cfg, _)) => {
            let port = cfg.dashboard.port;
            if port > 0 { port } else { fallback }
        }
        Err(_) => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_url_is_loopback_only() {
        let url = shutdown_url(9981);
        assert_eq!(url, "http://127.0.0.1:9981/api/admin/shutdown");
        assert!(url.contains("127.0.0.1"), "关停入口绝不能用可被解析到别处的名字");
    }

    #[test]
    fn min_version_is_the_one_that_shipped_the_endpoint() {
        // 与 NeoBot 侧 admin_shutdown 落地的版本保持一致；改这里要同时确认上游
        assert_eq!(MIN_VERSION_WITH_SHUTDOWN_ENDPOINT, "1.2.3");
    }
}
