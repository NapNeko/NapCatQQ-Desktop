//! QQ NT 登录账号探测（HotStart 模式自动匹配 PID 用）。
//!
//! QQ NT 启动后会在 127.0.0.1:9210-9219 任一端口监听一个迷你 HTTP 服务，处理
//! `tencent://` 深链接（浏览器点 QQ 群聊链接的回调）。POST /tencent body=
//! `tencent://snowluma-probe-noop` 会返回一段 JWT，base64url decode payload
//! 后得到当前登录的 uin / nickName。
//!
//! 协议信息来源:
//!   - SnowLuma `packages/core/src/hook/qq-port-probe.ts`（参考实装）
//!   - legacy Python `legacy-python/src/core/runtime/q_port_probe.py`
//!
//! body 用 `tencent://snowluma-probe-noop` 而不是裸 `tencent://`：legacy 注释
//! 里实测有些 QQ NT 版本把空 action 解析成"打开主窗口"会把 QQ 拉到前台，用
//! 一个 QQ 没注册的伪 action 让 deeplink dispatcher 静默丢弃，HTTP 层照常返
//! 回 JWT。
//!
//! 只对外暴露一个高层入口 [`find_pid_by_qq_id`]：枚举主 QQ.exe（过滤 Chromium
//! 子进程）→ 对每个 PID 在它实际监听的端口上做探测 → 找到 uin == qq_id 的 PID。
//! 找不到就返回 `None`，由调用方决定是报 InvalidConfig 还是其他降级路径。

use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use sysinfo::Pid;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const PROBE_PORT_START: u16 = 9210;
const PROBE_PORT_END: u16 = 9219;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// 单次端口探测的简化结果（只保留 backend 自动匹配关心的字段）。
#[derive(Debug, Clone)]
pub struct ProbedQqLogin {
    pub pid: u32,
    pub port: u16,
    pub uin: String,
}

/// 按 qq_id 自动匹配当前正在登录的 QQ.exe 主进程 PID。
///
/// 流程：
/// 1. sysinfo 枚举所有主 QQ.exe（排除 Chromium 子进程：parent 是 QQ.exe 或
///    cmdline 含 `--type=`）
/// 2. 并发对每个 PID 探测：先查 OS TCP 表拿 9210-9219 范围内该 PID 监听的端
///    口，再 POST `tencent://` payload 拿 JWT
/// 3. 找到 uin == qq_id 的就立即返回；全部探测完没匹配返回 None
///
/// `qq_id` 必须是十进制字符串形态（与 BotConfig.bot.qq_id 转换一致）。
pub async fn find_pid_by_qq_id(qq_id: u64) -> Option<ProbedQqLogin> {
    let target = qq_id.to_string();
    let pids = enumerate_main_qq_pids().await;
    if pids.is_empty() {
        return None;
    }
    // 并发探测：每个 PID 一个 task，先返回的命中即取
    let mut handles = Vec::with_capacity(pids.len());
    for pid in pids {
        handles.push(tokio::spawn(async move { probe_pid(pid).await }));
    }
    for handle in handles {
        match handle.await {
            Ok(Some(info)) if info.uin == target => return Some(info),
            _ => {}
        }
    }
    None
}

/// 枚举当前系统中所有"主"QQ.exe 进程，过滤 Chromium 子进程。
///
/// 过滤规则（与 legacy Python `enumerate_qq_processes` 同款）：
/// 1. parent name 也是 `QQ.exe` → Chromium fork 的子进程
/// 2. cmdline 任意 arg 含 `--type=` → Chromium 标 renderer / GPU / utility
async fn enumerate_main_qq_pids() -> Vec<u32> {
    tokio::task::spawn_blocking(|| {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        let qq_pids: std::collections::HashSet<Pid> = sys
            .processes()
            .iter()
            .filter(|(_, p)| p.name().to_string_lossy().eq_ignore_ascii_case("QQ.exe"))
            .map(|(pid, _)| *pid)
            .collect();

        let mut out: Vec<u32> = Vec::new();
        for (pid, process) in sys.processes().iter() {
            if !process.name().to_string_lossy().eq_ignore_ascii_case("QQ.exe") {
                continue;
            }
            // 规则 1：parent 也是 QQ.exe → Chromium 子进程
            if let Some(ppid) = process.parent() {
                if qq_pids.contains(&ppid) {
                    continue;
                }
            }
            // 规则 2：cmdline 含 `--type=`
            let has_chromium_type = process
                .cmd()
                .iter()
                .any(|s| s.to_string_lossy().contains("--type="));
            if has_chromium_type {
                continue;
            }
            out.push(pid.as_u32());
        }
        out.sort_unstable();
        out
    })
    .await
    .unwrap_or_default()
}

/// 对单个 PID 做完整探测：先查它监听的端口（缩小到 9210-9219），再发探测。
/// 拿不到端口就 fallback 全端口扫——这种 fallback 可能拿到别的 QQ 实例的 uin
/// 调用方会用 uin 校验过滤掉串号情形。
async fn probe_pid(pid: u32) -> Option<ProbedQqLogin> {
    let pid_for_blocking = pid;
    let pid_ports = tokio::task::spawn_blocking(move || listening_ports_for_pid(pid_for_blocking))
        .await
        .unwrap_or_default();

    if !pid_ports.is_empty() {
        for port in &pid_ports {
            if let Some(info) = probe_one_port(pid, *port).await {
                return Some(info);
            }
        }
        return None;
    }

    // PID-端口映射不可用（权限不足 / netstat 失败）→ 全端口 fallback
    for port in PROBE_PORT_START..=PROBE_PORT_END {
        if let Some(info) = probe_one_port(pid, port).await {
            return Some(info);
        }
    }
    None
}

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
        // listening 状态判断：netstat2 在 Windows 给 LISTENING；Linux 给 LISTEN。
        // 字符串包含比 enum 匹配更稳（变体名跨平台不一致）。
        let state_str = format!("{:?}", tcp.state).to_uppercase();
        if !state_str.contains("LISTEN") {
            continue;
        }
        if (PROBE_PORT_START..=PROBE_PORT_END).contains(&tcp.local_port) {
            ports.push(tcp.local_port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

async fn probe_one_port(pid: u32, port: u16) -> Option<ProbedQqLogin> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream =
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr)).await {
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
    let _ = stream.shutdown().await; // shutdown 失败不阻断后续读取

    let mut buf = Vec::with_capacity(2048);
    let _ = tokio::time::timeout(READ_TIMEOUT, stream.read_to_end(&mut buf)).await;
    let response = String::from_utf8_lossy(&buf);
    let jwt = extract_jwt(&response)?;
    let payload = decode_jwt_payload(jwt)?;
    if payload.err_code != 0 {
        return None;
    }
    let uin = payload
        .uin
        .or_else(|| payload.data.and_then(|d| d.uin))
        .unwrap_or_default();
    if uin.is_empty() {
        return None;
    }
    Some(ProbedQqLogin { pid, port, uin })
}

/// 在响应文本里搜第一段 JWT（三段式 base64url + 点分隔）。
/// 不引入 regex，手写扫描：找以 `eyJ` 开头的 token，到第二个 `.` 后的非
/// base64url 字符停下。base64url 字符集：A-Z a-z 0-9 - _
fn extract_jwt(text: &str) -> Option<&str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// JWT payload 解码：标准 base64url 用例。这条字面量是从 NapCat 实际响应里
    /// 截取的 payload 二段，errCode=0，uin=572381217。
    #[test]
    fn decode_jwt_payload_extracts_uin() {
        // payload = {"errCode":0,"uin":"572381217"}
        // base64url(unpadded) = eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ
        let token = "eyJ.eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ.sig";
        let payload = decode_jwt_payload(token).expect("decode payload");
        assert_eq!(payload.err_code, 0);
        assert_eq!(payload.uin.as_deref(), Some("572381217"));
    }

    /// extract_jwt 应当从混杂的 HTTP 响应文本里精确切出第一段 JWT。
    #[test]
    fn extract_jwt_finds_first_three_segment_token() {
        let body = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n\
                    {\"data\":\"eyJh.eyJiIjoxfQ.s_ig\"}";
        let jwt = extract_jwt(body).expect("jwt match");
        assert_eq!(jwt, "eyJh.eyJiIjoxfQ.s_ig");
    }

    /// 没有合法 JWT 时返回 None，不抛错。
    #[test]
    fn extract_jwt_returns_none_when_absent() {
        assert!(extract_jwt("plain text without token").is_none());
        // 只有两段不算 JWT
        assert!(extract_jwt("eyJa.eyJb").is_none());
    }
}
