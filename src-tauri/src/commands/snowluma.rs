//! SnowLuma Tauri commands。
//! 薄壳层：仅做参数转换 + 错误转 String + 调 BotManager / 实用工具
//! 。

use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use ts_rs::TS;

use crate::AppState;

fn snowluma_app_config_path(data_root: &std::path::Path) -> std::path::PathBuf {
    data_root.join("snowluma").join("app-config.json")
}

fn read_snowluma_app_config(data_root: &std::path::Path) -> ncd_runtime::SnowLumaAppConfig {
    let path = snowluma_app_config_path(data_root);
    if !path.exists() {
        return ncd_runtime::SnowLumaAppConfig::default();
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return ncd_runtime::SnowLumaAppConfig::default(),
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_snowluma_app_config(
    data_root: &std::path::Path,
    cfg: &ncd_runtime::SnowLumaAppConfig,
) -> Result<(), String> {
    let path = snowluma_app_config_path(data_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create snowluma dir failed: {e}"))?;
    }
    let text = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("serialize snowluma app config: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| format!("write snowluma app config tmp: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename snowluma app config: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn get_snowluma_app_config(
    state: State<'_, AppState>,
) -> Result<ncd_runtime::SnowLumaAppConfig, String> {
    Ok(read_snowluma_app_config(&state.data_root))
}

#[tauri::command]
pub async fn set_snowluma_app_config(
    state: State<'_, AppState>,
    config: ncd_runtime::SnowLumaAppConfig,
) -> Result<(), String> {
    write_snowluma_app_config(&state.data_root, &config)
}

/// 前端用来填 HOT 模式 attach_pid 时的 QQ 进程信息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct QQProcessInfo {
    pub pid: u32,
    pub name: String,
    pub started_at: u64,
    pub command_line: String,
}

/// 列出当前系统中所有"主"`QQ.exe` 进程（HOT 模式选 attach_pid 用）。
///
/// QQ NT 基于 Electron / Chromium，单次启动一个登录账号会派生十几个 `QQ.exe`
/// 子进程（renderer / GPU / utility / crash-handler …），如果把它们全列出
/// 同一个 uin 会出现 4-5 次重复条目。这里复用 legacy Python
/// (`legacy-python/src/core/runtime/q_port_probe.py`) 的两条 Chromium 子进程
/// 识别规则做过滤：
///
/// 1. parent name 也是 `QQ.exe` → Chromium fork 的子进程
/// 2. cmdline 含 `--type=` → Chromium 用此参数标 renderer / GPU / utility 等
///    子进程；也覆盖少数 parent 探测不到（权限不足等）的情形
#[tauri::command]
pub async fn list_qq_processes(state: State<'_, AppState>) -> Result<Vec<QQProcessInfo>, String> {
    let _ = state; // app state 当前未参与；未来若 backend 提供更精确视图再接入。
    let result = tokio::task::spawn_blocking(|| {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        // 第一遍收集所有 QQ.exe 的 PID，第二遍据此判断 parent 是不是子进程
        // 中转。两遍扫描而不是一遍 + lookup，因为 sysinfo 的 `process(parent_pid)`
        // 在某些系统上对已退出 / 权限不足的 PID 返回 None，第一遍存集合最稳。
        let mut qq_pids: std::collections::HashSet<sysinfo::Pid> =
            std::collections::HashSet::new();
        for (pid, process) in sys.processes().iter() {
            if process.name().to_string_lossy().eq_ignore_ascii_case("QQ.exe") {
                qq_pids.insert(*pid);
            }
        }

        let mut out: Vec<QQProcessInfo> = Vec::new();
        for (pid, process) in sys.processes().iter() {
            let name = process.name().to_string_lossy().to_string();
            if !name.eq_ignore_ascii_case("QQ.exe") {
                continue;
            }

            // 规则 1：parent 也是 QQ.exe → Chromium 子进程
            if let Some(ppid) = process.parent() {
                if qq_pids.contains(&ppid) {
                    continue;
                }
            }

            let cmd: Vec<String> = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect();

            // 规则 2：cmdline 含 `--type=` → Chromium 子进程
            if cmd.iter().any(|arg| arg.contains("--type=")) {
                continue;
            }

            out.push(QQProcessInfo {
                pid: pid.as_u32(),
                name,
                started_at: process.start_time(),
                command_line: cmd.join(" "),
            });
        }
        // 按 PID 升序，让 UI 列表稳定
        out.sort_by_key(|p| p.pid);
        out
    })
    .await
    .map_err(|e| format!("list_qq_processes spawn_blocking failed: {e}"))?;
    Ok(result)
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
    let path = snowluma_app_config_path(&state.data_root);
    let mut cfg = read_snowluma_app_config(&state.data_root);
    cfg.webui_password_override = password.unwrap_or_default();
    write_snowluma_app_config(&state.data_root, &cfg)
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

    // 端口：daemon 写入的 runtime.json 优先；否则读 app-config.json；再默认 5099。
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
    .unwrap_or_else(|| {
        ncd_runtime::load_snowluma_app_config(&data_root.join("snowluma")).webui_port
    });

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


// =============================================================================
// QQ 登录账号探测（QQ NT tencent:// HTTP 端点 9210-9219）
// =============================================================================
//
// QQ NT 启动后会在 127.0.0.1:9210-9219 任一端口监听一个迷你 HTTP 服务，处理
// `tencent://` 深链接（浏览器点 QQ 群聊链接的回调）。POST /tencent body=`tencent://`
// 会返回一段 JWT，base64url decode payload 后得到当前登录的 uin / nickName。
//
// 协议信息来源:
//   - SnowLuma `packages/core/src/hook/qq-port-probe.ts` （参考实装）
//   - legacy Python `legacy-python/src/core/runtime/q_port_probe.py`
//
// body 用 `tencent://snowluma-probe-noop` 而不是裸 `tencent://`：legacy 注释里
// 实测有些 QQ NT 版本把空 action 解析成"打开主窗口"会把 QQ 拉到前台，用一个
// QQ 没注册的伪 action 让 deeplink dispatcher 静默丢弃，HTTP 层照常返回 JWT。

const QQ_PROBE_PORT_START: u16 = 9210;
const QQ_PROBE_PORT_END: u16 = 9219;
const QQ_PROBE_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const QQ_PROBE_READ_TIMEOUT: Duration = Duration::from_secs(1);

/// 单条 QQ 登录探测结果。`logged_in == false` 表示端口响应了但当前未登录。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct QqLoginInfo {
    pub port: u16,
    pub uin: String,
    pub uid: String,
    pub nickname: String,
    pub logged_in: bool,
}

/// 探测指定 PID 当前登录的 QQ 账号。
///
/// 关键路径：先用 OS TCP 表查 PID 实际监听的端口（缩小到该 QQ NT 实例自己的
/// 端口，避免跨实例串号），再对该端口发 tencent:// 探测。如果 TCP 表查不到
/// （权限不足 / 进程刚退出），fallback 到全端口扫——这种 fallback 拿到的可能
/// 是别的 QQ NT 实例的 uin，UI 层应当容忍并由用户确认。
///
/// 失败原因（任意一种都返回 `None`）：
/// - 进程已退出 / 端口未开放
/// - 9210-9219 全部不响应
/// - JWT 解码失败 / payload errCode != 0
///
/// 阻塞操作（TCP connect / read），但跑在 tokio 异步任务里，不卡主循环。
#[tauri::command]
pub async fn probe_qq_login_info(
    state: State<'_, AppState>,
    pid: u32,
) -> Result<Option<QqLoginInfo>, String> {
    let _ = state;
    if pid == 0 {
        return Ok(None);
    }

    // 1. 优先扫 PID 实际监听的 9210-9219 范围内端口（精确路径，不会串号）
    let pid_ports = tokio::task::spawn_blocking(move || listening_ports_for_pid(pid))
        .await
        .unwrap_or_default();
    for port in &pid_ports {
        if let Some(info) = probe_one_port(*port).await {
            return Ok(Some(info));
        }
    }

    // 2. fallback：PID 端口拿不到时退到全端口扫描
    //    （legacy Python / SnowLuma 都保留了这条 fallback；权限不足时常见）
    if pid_ports.is_empty() {
        for port in QQ_PROBE_PORT_START..=QQ_PROBE_PORT_END {
            if let Some(info) = probe_one_port(port).await {
                return Ok(Some(info));
            }
        }
    }
    Ok(None)
}

/// 通过 OS TCP 表查 PID 监听的端口，过滤到 9210-9219 范围内。
/// 失败返回空向量，让上层走全端口 fallback。
fn listening_ports_for_pid(pid: u32) -> Vec<u16> {
    use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;
    let sockets = match get_sockets_info(af_flags, proto_flags) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut ports: Vec<u16> = Vec::new();
    for socket in sockets {
        if !socket.associated_pids.contains(&pid) {
            continue;
        }
        let ProtocolSocketInfo::Tcp(tcp) = socket.protocol_socket_info else {
            continue;
        };
        // listening 状态：netstat2 在 Windows 给 LISTENING；Linux 给 LISTEN。
        // 用字符串包含判断比 enum 匹配更稳（不同平台变体名差异大）。
        let state_str = format!("{:?}", tcp.state).to_uppercase();
        if !state_str.contains("LISTEN") {
            continue;
        }
        if (QQ_PROBE_PORT_START..=QQ_PROBE_PORT_END).contains(&tcp.local_port) {
            ports.push(tcp.local_port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

async fn probe_one_port(port: u16) -> Option<QqLoginInfo> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = match tokio::time::timeout(
        QQ_PROBE_CONNECT_TIMEOUT,
        TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(s)) => s,
        _ => return None,
    };

    let body = "tencent://snowluma-probe-noop";
    let request = format!(
        "POST /tencent HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(request.as_bytes()).await.is_err() {
        return None;
    }
    if stream.shutdown().await.is_err() {
        // shutdown 失败不影响读取，继续。
    }

    let mut buf = Vec::with_capacity(2048);
    let _ = tokio::time::timeout(QQ_PROBE_READ_TIMEOUT, stream.read_to_end(&mut buf)).await;
    let response = String::from_utf8_lossy(&buf);
    let jwt = extract_jwt(&response)?;
    let payload = decode_jwt_payload(jwt)?;
    if payload.err_code != 0 {
        return None;
    }
    let uin = payload
        .uin
        .clone()
        .or_else(|| payload.data.as_ref().and_then(|d| d.uin.clone()))
        .unwrap_or_default();

    Some(QqLoginInfo {
        port,
        logged_in: !uin.is_empty(),
        uin,
        uid: payload.uid.unwrap_or_default(),
        nickname: payload.nick_name.unwrap_or_default(),
    })
}

/// 在响应文本里搜第一段 JWT（三段式 base64url + 点分隔）。
fn extract_jwt(text: &str) -> Option<&str> {
    // 不引入 regex，手写扫描：找以 `eyJ` 开头的 token，到第二个 `.` 后的非
    // base64url 字符停下。base64url 字符集：A-Z a-z 0-9 - _
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"eyJ" {
            let start = i;
            let mut j = i;
            let mut dots = 0;
            while j < bytes.len() {
                let b = bytes[j];
                let is_b64url = b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
                if is_b64url {
                    j += 1;
                    continue;
                }
                if b == b'.' {
                    dots += 1;
                    j += 1;
                    if dots == 3 {
                        // 走过了第三段；跑到 4 个 `.` 实际不存在，这分支保留鲁棒。
                        break;
                    }
                    continue;
                }
                break;
            }
            if dots == 2 {
                let s = &text[start..j];
                if s.contains('.') {
                    return Some(s);
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    None
}

#[derive(Debug, Deserialize)]
struct JwtPayload {
    #[serde(default, rename = "errCode")]
    err_code: i32,
    #[serde(default)]
    uin: Option<String>,
    #[serde(default)]
    uid: Option<String>,
    #[serde(default, rename = "nickName")]
    nick_name: Option<String>,
    #[serde(default)]
    data: Option<JwtPayloadData>,
}

#[derive(Debug, Deserialize)]
struct JwtPayloadData {
    #[serde(default)]
    uin: Option<String>,
}

fn decode_jwt_payload(token: &str) -> Option<JwtPayload> {
    let segment = token.split('.').nth(1)?;
    // JWT 通常省略 base64url 尾部 `=` padding，手动补齐。
    let mut padded = segment.to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let bytes = base64::engine::general_purpose::URL_SAFE.decode(padded).ok()?;
    serde_json::from_slice(&bytes).ok()
}
