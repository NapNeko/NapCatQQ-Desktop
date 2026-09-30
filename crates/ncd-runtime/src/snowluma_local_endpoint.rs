//! 本机 SnowLuma daemon 的 WebUI 端口和登录密码。
//!
//! 「打开 SnowLuma WebUI」按钮和调试台的 SnowLuma 内部通道都要它，所以放在运行时这一层，
//! 两边读到的永远是同一个端口、同一个密码。本机 daemon 是全局单例，所有本机 SnowLuma Bot
//! 共用这一个端点。

use std::path::Path;

use ncd_domain::SnowLumaAppConfig;

use crate::data_paths::DataPaths;
use crate::snowluma::load_snowluma_app_config;

/// 解析本机 SnowLuma WebUI 的 `(端口, 密码)`。
///
/// 端口：daemon 写入的 `runtime.json` 的 `webuiPort` 优先（端口被占时 daemon 会换口，
/// 只有这份记录是真的）；读不到再用 `app-config.json` 里的配置值（缺省 5099）。
///
/// 密码（与 daemon `render_daemon_globals` 的优先级一致）：
/// 1. App 级覆盖：`app-config.json` 的 `webuiPasswordOverride`，去掉首尾空白后非空；
/// 2. 否则是 daemon 首次启动时写进 `session.json` 的强随机密码。
///
/// `session.json` 还没生成（从没启动过 SnowLuma Bot）时返回给人看的错误文案。
pub fn local_snowluma_webui_endpoint(data_root: &Path) -> Result<(u16, String), String> {
    let paths = DataPaths::new(data_root);
    let runtime_json_path = paths.snowluma_config_dir().join("runtime.json");
    let port: u16 = (|| -> Option<u16> {
        let text = std::fs::read_to_string(&runtime_json_path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.get("webuiPort")
            .and_then(|v| v.as_u64())
            .map(|n| n as u16)
    })()
    .unwrap_or_else(|| load_snowluma_app_config(&paths.snowluma_data_dir()).webui_port);

    let app_cfg_path = paths.snowluma_data_dir().join("app-config.json");
    let override_pwd: Option<String> = (|| -> Option<String> {
        let text = std::fs::read_to_string(&app_cfg_path).ok()?;
        let cfg: SnowLumaAppConfig = serde_json::from_str(&text).ok()?;
        let trimmed = cfg.webui_password_override.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })();

    let password = match override_pwd {
        Some(p) => p,
        None => {
            // session.json 由 daemon 启动时写入；文件还没生成就明确告诉用户先启动一次
            let session_path = paths.snowluma_data_dir().join("session.json");
            let text = std::fs::read_to_string(&session_path).map_err(|e| {
                format!("SnowLuma session 未就绪（请先启动至少一个 SnowLuma Bot）：{e}")
            })?;
            let session: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("解析 session.json 失败：{e}"))?;
            session
                .get("password")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "session.json 缺少 password 字段".to_string())?
                .to_string()
        }
    };

    Ok((port, password))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn runtime_port_and_session_password_are_used() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(dir.path());
        write(
            &paths.snowluma_config_dir().join("runtime.json"),
            r#"{"webuiPort": 5123}"#,
        );
        write(
            &paths.snowluma_data_dir().join("session.json"),
            r#"{"password": "from-session"}"#,
        );

        let (port, password) = local_snowluma_webui_endpoint(dir.path()).unwrap();
        assert_eq!(port, 5123);
        assert_eq!(password, "from-session");
    }

    #[test]
    fn app_config_override_wins_over_session_and_supplies_port() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(dir.path());
        let cfg = SnowLumaAppConfig {
            webui_port: 5200,
            webui_password_override: "  manual  ".into(),
        };
        write(
            &paths.snowluma_data_dir().join("app-config.json"),
            &serde_json::to_string(&cfg).unwrap(),
        );
        write(
            &paths.snowluma_data_dir().join("session.json"),
            r#"{"password": "from-session"}"#,
        );

        let (port, password) = local_snowluma_webui_endpoint(dir.path()).unwrap();
        assert_eq!(port, 5200, "没有 runtime.json 时用 app-config 里的端口");
        assert_eq!(password, "manual");
    }

    #[test]
    fn missing_session_is_a_readable_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = local_snowluma_webui_endpoint(dir.path()).unwrap_err();
        assert!(err.starts_with("SnowLuma session 未就绪"), "{err}");

        let paths = DataPaths::new(dir.path());
        write(&paths.snowluma_data_dir().join("session.json"), r#"{}"#);
        let err = local_snowluma_webui_endpoint(dir.path()).unwrap_err();
        assert_eq!(err, "session.json 缺少 password 字段");
    }
}
