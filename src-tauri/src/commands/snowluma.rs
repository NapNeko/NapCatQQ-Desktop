//! SnowLuma Tauri commands。
//! 薄壳层：仅做参数转换 + 错误转 String + 调 BotManager / 实用工具
//! 。

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::AppState;

/// 前端用来填 HOT 模式 attach_pid 时的 QQ 进程信息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct QQProcessInfo {
    pub pid: u32,
    pub name: String,
    pub started_at: u64,
    pub command_line: String,
}

/// 列出当前系统中所有名为 `QQ.exe` 的进程（HOT 模式选 attach_pid 用）。
#[tauri::command]
pub async fn list_qq_processes(state: State<'_, AppState>) -> Result<Vec<QQProcessInfo>, String> {
    let _ = state; // app state 当前未参与；未来若 backend 提供更精确视图再接入。
    let result = tokio::task::spawn_blocking(|| {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let mut out: Vec<QQProcessInfo> = Vec::new();
        for (pid, process) in sys.processes().iter() {
            let name = process.name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("QQ.exe") {
                let cmd: Vec<String> = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect();
                out.push(QQProcessInfo {
                    pid: pid.as_u32(),
                    name,
                    started_at: process.start_time(),
                    command_line: cmd.join(" "),
                });
            }
        }
        out
    })
    .await
    .map_err(|e| format!("list_qq_processes spawn_blocking failed: {e}"))?;
    Ok(result)
}

/// 把 `attach_pid` 写入对应 BotConfig 的 `snowluma_start_mode = HotStart{..}`。
#[tauri::command]
pub async fn set_snowluma_attach_pid(
    state: State<'_, AppState>,
    bot_id: String,
    attach_pid: u32,
) -> Result<(), String> {
    use ncd_runtime::{BotId, SnowLumaStartMode};
    let bot_id = BotId::new(bot_id);
    let mut config = state
        .bot_manager
        .get_bot_config(&bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("bot config not found: {bot_id}"))?;
    config.bot.snowluma_start_mode = Some(SnowLumaStartMode::HotStart { attach_pid });
    state
        .bot_manager
        .upsert_bot_config(config)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 更新 App 级 SnowLuma WebUI 密码 override。
/// MVP：当前仅原子写 `<data_root>/snowluma/app-config.json`，下次 daemon 启动时
/// 由 `render_daemon_globals` 读取该 override。配置层热更新未来
/// 可通过 `BotManager` 内的 `Arc<RwLock<SnowLumaAppConfig>>` 接入。
#[tauri::command]
pub async fn set_snowluma_password_override(
    state: State<'_, AppState>,
    password: Option<String>,
) -> Result<(), String> {
    use ncd_runtime::SnowLumaAppConfig;
    let path = state.data_root.join("snowluma").join("app-config.json");
    let mut cfg: SnowLumaAppConfig = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("read app-config.json failed: {e}"))?;
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        SnowLumaAppConfig::default()
    };
    cfg.webui_password_override = password.unwrap_or_default();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create snowluma dir failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&cfg)
        .map_err(|e| format!("serialize snowluma app config: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| format!("write snowluma app config tmp: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename snowluma app config: {e}"))?;
    Ok(())
}

/// SnowLuma WebUI 登录端点。
/// SnowLuma WebUI 走**表单登录**，不接受 `?token=` query 参数。前端拿到这个
/// payload 后：
/// 1. 把 `password` 写入剪贴板，提示「已复制到剪贴板，粘贴即可登录」
/// 2. 用系统默认浏览器打开 `url`。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct SnowLumaWebuiEndpoint {
    pub url: String,
    pub password: String,
}

/// 打开 SnowLuma WebUI：解析 endpoint URL + 当前生效的 password。
/// 优先级（与 daemon `render_daemon_globals` 对齐）：
/// 1. App 级 override（`<data_root>/snowluma/app-config.json` 的
/// `snowlumaWebuiPasswordOverride`，非空）。
/// 2. session.json 中的强随机密码。
/// `bot_id` 当前未使用——SnowLuma daemon 是全局单例，所有 SL bot 共享同一个
/// WebUI 端点。保留参数是为了未来支持每 Bot 隔离时改造方便。
#[tauri::command]
pub async fn open_snowluma_webui(
    state: State<'_, AppState>,
    _bot_id: String,
) -> Result<SnowLumaWebuiEndpoint, String> {
    use ncd_runtime::SnowLumaAppConfig;

    let data_root = state.data_root.clone();

    // 端口：先读 SnowLuma 安装目录下 `config/runtime.json` 的 `webuiPort`
    // （由 daemon `render_runtime_json` 写入）；读不到就回落到 5099。
    let runtime_json_path = data_root
        .join("runtime")
        .join("SnowLuma")
        .join("config")
        .join("runtime.json");
    let port: u16 = (|| -> Option<u16> {
        let text = std::fs::read_to_string(&runtime_json_path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.get("webuiPort")
            .and_then(|v| v.as_u64())
            .map(|n| n as u16)
    })()
    .unwrap_or_else(ncd_runtime::default_snowluma_port);

    // 密码：先看 App-level override，否则读 session.json。
    let app_cfg_path = data_root.join("snowluma").join("app-config.json");
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
            // session.json 由 daemon 启动时写入；如果文件还没生成（用户没启动过
            // SL bot），返回明确错误让前端提示。
            let session_path = data_root.join("snowluma").join("session.json");
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

    Ok(SnowLumaWebuiEndpoint {
        url: format!("http://127.0.0.1:{port}/"),
        password,
    })
}
