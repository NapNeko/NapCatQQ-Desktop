//! QQ NT 登录账号探测(HotStart 模式自动匹配 PID 用)
//!
//! 给定一个 qq_id,在本机找到正登录该账号的 QQ.exe 主进程 PID两级探测:
//!
//! 主路径 —— Ptlogin2 无弹窗探测QQ NT 为网页端快捷登录在本地起一个 HTTPS
//! 服务,监听 4301/4303/4305/4307/4309 之一GET
//! https://127.0.0.1:<port>/pt_get_uins(伪造 Host/Referer/Cookie,自签证书
//! 忽略校验)拿回一段 JSONP,里面是已登录账号列表全程不碰深链接,QQ 不弹窗
//!
//! 这个接口有个怪脾气:它交替返回"最近两个登录过的账号"和"当前账号"所以连
//! 发两次取较短的一组——较短组恰好 1 个时即当前账号,否则放弃转兜底
//!
//! 兜底 —— 旧的 tencent:// 深链接探测POST /tencent body=
//! tencent://snowluma-probe-noop 到该进程实际监听的 9210-9219 端口,返回的
//! JWT 里有 uin这条会让个别 QQ 版本弹"深链接解析失败",所以只在 Ptlogin2
//! 落空时按 PID 实际端口兜底,不再无差别全段盲扫(盲扫正是上游 PR #73 修掉的
//! 痛点:既打扰用户又没和指定 PID 严格绑定)
//!
//! body 用伪 action tencent://snowluma-probe-noop 而非裸 tencent://:实测
//! 空 action 会被某些 QQ 版本解析成"打开主窗口"把 QQ 拉到前台,伪 action 让
//! deeplink dispatcher 静默丢弃,HTTP 层照常返回 JWT
//!
//! 协议来源:
//!   - SnowLuma packages/bridge/src/qq-port-probe.ts(上游 PR #73 实装)
//!   - legacy Python legacy-python/src/core/runtime/q_port_probe.py
//!
//! 只对外暴露一个高层入口 [find_pid_by_qq_id]:枚举主 QQ.exe(过滤 Chromium
//! 子进程)→ 对每个 PID 先 Ptlogin2 后深链接探测 → 找到 uin == qq_id 的 PID
//! 找不到就返回 None,由调用方决定是报 InvalidConfig 还是其他降级路径

use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use sysinfo::Pid;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 深链接兜底探测的端口范围(QQ NT 处理 tencent:// 的迷你 HTTP 服务)
const PROBE_PORT_START: u16 = 9210;
const PROBE_PORT_END: u16 = 9219;
/// QQ NT 网页端快捷登录(Ptlogin2)本地 HTTPS 服务的候选端口
const PT_LOGIN_PORTS: [u16; 5] = [4301, 4303, 4305, 4307, 4309];
/// 进程数启发式阈值:5 个 Ptlogin 端口用满需要 5 个独立 QQ,加上各自的
/// Chromium 子进程总数会远超此值低于阈值时推断当前是"唯一实例等待登录",
/// 直接返回未登录而不去发深链接(避免无谓弹窗)与上游 PR #73 取值一致
const QQ_PROCESS_POLLUTION_THRESHOLD: usize = 6;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const READ_TIMEOUT: Duration = Duration::from_secs(1);
/// 单次 Ptlogin2 HTTPS 请求超时本地回环 + 自签证书握手,给足 1.5s
const PTLOGIN_TIMEOUT: Duration = Duration::from_millis(1500);

/// 单次端口探测的简化结果(只保留 backend 自动匹配关心的字段)
#[derive(Debug, Clone)]
pub struct ProbedQqLogin {
    pub pid: u32,
    pub port: u16,
    pub uin: String,
}

/// 按 qq_id 自动匹配当前正在登录的 QQ.exe 主进程 PID
///
/// 流程:
/// 1. sysinfo 枚举所有主 QQ.exe(排除 Chromium 子进程:parent 是 QQ.exe 或
///    cmdline 含 --type=)
/// 2. 并发对每个 PID 探测:先查 OS TCP 表拿该 PID 实际监听的端口,命中 Ptlogin
///    端口走无弹窗 Ptlogin2 探测,落空再按 9210-9219 深链接兜底
/// 3. 找到 uin == qq_id 的就立即返回;全部探测完没匹配返回 None
///
/// qq_id 必须是十进制字符串形态(与 BotConfig.bot.qq_id 转换一致)
pub async fn find_pid_by_qq_id(qq_id: u64) -> Option<ProbedQqLogin> {
    let target = qq_id.to_string();
    let scan = scan_qq_processes().await;
    if scan.main_pids.is_empty() {
        return None;
    }
    let total_qq_count = scan.total_qq_count;
    // 并发探测:每个 PID 一个 task,先返回的命中即取
    let mut handles = Vec::with_capacity(scan.main_pids.len());
    for pid in scan.main_pids {
        handles.push(tokio::spawn(
            async move { probe_pid(pid, total_qq_count).await },
        ));
    }
    for handle in handles {
        if let Ok(Some(info)) = handle.await {
            if info.uin == target {
                return Some(info);
            }
        }
    }
    None
}

/// 一次 sysinfo 扫描的结果:主 QQ.exe PID 列表 + QQ.exe 进程总数
///
/// 进程总数把 Chromium 子进程也算进去,用于 [probe_pid] 的污染启发式
/// (对齐上游 PR #73 的 getQqProcessCount)复用同一次 refresh_all
/// 而非另起 tasklist/pgrep,省一次进程枚举
struct QqProcessScan {
    main_pids: Vec<u32>,
    total_qq_count: usize,
}

/// 枚举当前系统中所有"主"QQ.exe 进程并统计 QQ.exe 总数,过滤 Chromium 子进程
///
/// 过滤规则(与 legacy Python enumerate_qq_processes 同款):
/// 1. parent name 也是 QQ.exe → Chromium fork 的子进程
/// 2. cmdline 任意 arg 含 --type= → Chromium 标 renderer / GPU / utility
async fn scan_qq_processes() -> QqProcessScan {
    tokio::task::spawn_blocking(|| {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        let is_qq = |p: &sysinfo::Process| p.name().to_string_lossy().eq_ignore_ascii_case("QQ.exe");

        let qq_pids: std::collections::HashSet<Pid> = sys
            .processes()
            .iter()
            .filter(|(_, p)| is_qq(p))
            .map(|(pid, _)| *pid)
            .collect();
        let total_qq_count = qq_pids.len();

        let mut main_pids: Vec<u32> = Vec::new();
        for (pid, process) in sys.processes().iter() {
            if !is_qq(process) {
                continue;
            }
            // 规则 1:parent 也是 QQ.exe → Chromium 子进程
            if let Some(ppid) = process.parent() {
                if qq_pids.contains(&ppid) {
                    continue;
                }
            }
            // 规则 2:cmdline 含 --type=
            let has_chromium_type = process
                .cmd()
                .iter()
                .any(|s| s.to_string_lossy().contains("--type="));
            if has_chromium_type {
                continue;
            }
            main_pids.push(pid.as_u32());
        }
        main_pids.sort_unstable();
        QqProcessScan {
            main_pids,
            total_qq_count,
        }
    })
    .await
    .unwrap_or(QqProcessScan {
        main_pids: Vec::new(),
        total_qq_count: 0,
    })
}

/// 对单个 PID 做完整探测total_qq_count 是全系统 QQ.exe 进程数(含 Chromium
/// 子进程),用于污染启发式判断
///
/// 两级策略(对齐上游 PR #73):
/// 1. 拿该 PID 实际监听的全部端口;命中 4301/4303/4305/4307/4309 任一就走
///    Ptlogin2 无弹窗探测,成功即返回
/// 2. Ptlogin2 落空(或没开快捷登录端口),按该 PID 实际监听的 9210-9219 端口
///    做深链接兜底深链接只打该 PID 自己的端口,不再无差别全段盲扫
///
/// 启发式:若没拿到任何端口,或没开快捷登录端口,且全系统 QQ.exe 进程数低于
/// 阈值,则推断这是唯一一个还停在登录界面的实例 —— 没东西可读,直接放弃,
/// 不去发深链接(避免无谓弹窗)进程数超阈值说明环境复杂(多实例/残留),
/// 才放行到深链接兜底
async fn probe_pid(pid: u32, total_qq_count: usize) -> Option<ProbedQqLogin> {
    let ports = tokio::task::spawn_blocking(move || listening_ports_for_pid(pid))
        .await
        .unwrap_or_default();

    if ports.is_empty() {
        // 没拿到端口:要么进程没起 TCP 服务(还在登录界面),要么 netstat 拿不到
        // 映射低于阈值当作"唯一实例等待登录"放弃;超阈值环境复杂也不盲扫,
        // 因为没有 PID→端口绑定,盲扫 9210-9219 会打到别的实例还触发弹窗
        return None;
    }

    // 主路径:Ptlogin2 无弹窗探测命中的快捷登录端口
    let pt_ports: Vec<u16> = ports
        .iter()
        .copied()
        .filter(|p| PT_LOGIN_PORTS.contains(p))
        .collect();
    if !pt_ports.is_empty() {
        // 多端口取一个客户端复用 TLS/连接池
        let client = build_ptlogin_client()?;
        for port in &pt_ports {
            if let Some(info) = try_ptlogin(&client, pid, *port).await {
                return Some(info);
            }
        }
    } else if total_qq_count < QQ_PROCESS_POLLUTION_THRESHOLD {
        // 没开快捷登录端口 + 环境干净 → 唯一实例等待登录,不发深链接
        return None;
    }

    // 兜底:深链接探测,只打该 PID 自己监听的 9210-9219 端口
    let deep_link_ports = ports
        .iter()
        .copied()
        .filter(|p| (PROBE_PORT_START..=PROBE_PORT_END).contains(p));
    for port in deep_link_ports {
        if let Some(info) = probe_one_port(pid, port).await {
            return Some(info);
        }
    }
    None
}

/// 查 PID 实际监听的全部 TCP 端口(不再预先过滤到某个范围,由调用方分类)
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
        // listening 状态判断:netstat2 在 Windows 给 LISTENING;Linux 给 LISTEN
        // 字符串包含比 enum 匹配更稳(变体名跨平台不一致)
        let state_str = format!("{:?}", tcp.state).to_uppercase();
        if !state_str.contains("LISTEN") {
            continue;
        }
        ports.push(tcp.local_port);
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

// ---------------------------------------------------------------------------
// 主路径:Ptlogin2 无弹窗探测
// ---------------------------------------------------------------------------

/// Ptlogin2 /pt_get_uins 返回的单个账号条目(只关心 uin,nickName 等字段忽略)
/// uin 在不同 QQ 版本里可能是字符串也可能是数字,两者都接
#[derive(Debug, Deserialize)]
struct PtloginAccount {
    #[serde(default)]
    uin: Option<UinField>,
    #[serde(default)]
    account: Option<UinField>,
}

/// uin / account 字段的宽松类型:JSON 里给字符串或数字都能解
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UinField {
    Str(String),
    Num(u64),
}

impl UinField {
    fn as_string(&self) -> String {
        match self {
            UinField::Str(s) => s.clone(),
            UinField::Num(n) => n.to_string(),
        }
    }
}

impl PtloginAccount {
    /// 取 uin,空则回退 account(对齐上游 account.uin || account.account)
    fn uin_string(&self) -> String {
        if let Some(u) = self.uin.as_ref().map(UinField::as_string) {
            if !u.is_empty() {
                return u;
            }
        }
        self.account.as_ref().map(UinField::as_string).unwrap_or_default()
    }
}

/// 构造忽略自签证书的 HTTPS 客户端Ptlogin2 本地服务用 QQ 自签证书,必须
/// danger_accept_invalid_certs;本地回环不经代理,显式 no_proxy
fn build_ptlogin_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(PTLOGIN_TIMEOUT)
        .no_proxy()
        .build()
        .ok()
}

/// 双发包对齐:连发两次 /pt_get_uins,取较短的一组Ptlogin 接口交替返回
/// "最近两个登录过的账号"和"当前账号",较短组恰好 1 条时即当前登录账号;其余
/// 情况(2+0 / 都是 2 / 空)放弃,由调用方降级到深链接兜底
async fn try_ptlogin(client: &reqwest::Client, pid: u32, port: u16) -> Option<ProbedQqLogin> {
    let res1 = fetch_ptlogin(client, port).await;
    let res2 = fetch_ptlogin(client, port).await;
    let uin = select_current_uin(&res1, &res2)?;
    Some(ProbedQqLogin { pid, port, uin })
}

/// 双发包结果对齐的纯逻辑部分(抽出来便于单测)
fn select_current_uin(res1: &[PtloginAccount], res2: &[PtloginAccount]) -> Option<String> {
    let shorter = if res1.len() < res2.len() { res1 } else { res2 };
    if shorter.len() == 1 {
        let uin = shorter[0].uin_string();
        if !uin.is_empty() {
            return Some(uin);
        }
    }
    None
}

/// 向 https://127.0.0.1:<port>/pt_get_uins 发一次 GET,伪造请求头骗过本地
/// 服务的来源校验,解析 JSONP 返回账号列表任何错误都吞成空列表(探测语义)
async fn fetch_ptlogin(client: &reqwest::Client, port: u16) -> Vec<PtloginAccount> {
    let url = format!("https://127.0.0.1:{port}/pt_get_uins?callback=ptui_getuins_CB&pt_local_tk=0");
    let resp = match client
        .get(&url)
        .header("Host", "localhost.ptlogin2.qq.com")
        .header("Referer", "https://xui.ptlogin2.qq.com/")
        .header("Cookie", "pt_local_token=0")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let text = resp.text().await.unwrap_or_default();
    parse_ptlogin_accounts(&text)
}

/// 从 JSONP 文本里抠出账号数组复刻上游切片逻辑:取第一个 [ 到其后第一个
/// ] 之间的内容,包成数组解析形如
/// var var_sso_uin_list=[{...},{...}];ptui_getuins_CB(...)解析失败给空列表
fn parse_ptlogin_accounts(text: &str) -> Vec<PtloginAccount> {
    let Some(open) = text.find('[') else {
        return Vec::new();
    };
    let rest = &text[open + 1..];
    let Some(close) = rest.find(']') else {
        return Vec::new();
    };
    let inner = &rest[..close];
    serde_json::from_str::<Vec<PtloginAccount>>(&format!("[{inner}]")).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 兜底路径:tencent:// 深链接探测(旧机制,可能触发 QQ 弹窗)
// ---------------------------------------------------------------------------

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

/// 在响应文本里搜第一段 JWT(三段式 base64url + 点分隔)
/// 不引入 regex,手写扫描:找以 eyJ 开头的 token,到第二个 . 后的非
/// base64url 字符停下base64url 字符集:A-Z a-z 0-9 - _
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
    // JWT 通常省略 base64url 尾部 = padding,手动补齐
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

    /// JWT payload 解码:标准 base64url 用例这条字面量是从 NapCat 实际响应里
    /// 截取的 payload 二段,errCode=0,uin=572381217
    #[test]
    fn decode_jwt_payload_extracts_uin() {
        // payload = {"errCode":0,"uin":"572381217"}
        // base64url(unpadded) = eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ
        let token = "eyJ.eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ.sig";
        let payload = decode_jwt_payload(token).expect("decode payload");
        assert_eq!(payload.err_code, 0);
        assert_eq!(payload.uin.as_deref(), Some("572381217"));
    }

    /// extract_jwt 应当从混杂的 HTTP 响应文本里精确切出第一段 JWT
    #[test]
    fn extract_jwt_finds_first_three_segment_token() {
        let body = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n\
                    {\"data\":\"eyJh.eyJiIjoxfQ.s_ig\"}";
        let jwt = extract_jwt(body).expect("jwt match");
        assert_eq!(jwt, "eyJh.eyJiIjoxfQ.s_ig");
    }

    /// 没有合法 JWT 时返回 None,不抛错
    #[test]
    fn extract_jwt_returns_none_when_absent() {
        assert!(extract_jwt("plain text without token").is_none());
        // 只有两段不算 JWT
        assert!(extract_jwt("eyJa.eyJb").is_none());
    }

    /// JSONP 解析:从 Ptlogin2 真实返回形态里抠出账号数组
    #[test]
    fn parse_ptlogin_accounts_extracts_array() {
        let body = "var var_sso_uin_list=[{\"uin\":\"572381217\",\"nickname\":\"a\"}];\
                    ptui_getuins_CB(var_sso_uin_list)";
        let accts = parse_ptlogin_accounts(body);
        assert_eq!(accts.len(), 1);
        assert_eq!(accts[0].uin_string(), "572381217");
    }

    /// uin 为数字字面量也能解(不同 QQ 版本返回类型不一致)
    #[test]
    fn parse_ptlogin_accounts_accepts_numeric_uin() {
        let body = "x=[{\"uin\":572381217},{\"account\":10001}]";
        let accts = parse_ptlogin_accounts(body);
        assert_eq!(accts.len(), 2);
        assert_eq!(accts[0].uin_string(), "572381217");
        // 第二条 uin 缺失回退 account
        assert_eq!(accts[1].uin_string(), "10001");
    }

    /// 无方括号 / 非法 JSON → 空列表,不 panic
    #[test]
    fn parse_ptlogin_accounts_returns_empty_on_garbage() {
        assert!(parse_ptlogin_accounts("no brackets here").is_empty());
        assert!(parse_ptlogin_accounts("[not valid json}").is_empty());
        assert!(parse_ptlogin_accounts("").is_empty());
    }

    fn acct(uin: &str) -> PtloginAccount {
        PtloginAccount {
            uin: Some(UinField::Str(uin.to_string())),
            account: None,
        }
    }

    /// 双发包对齐:情况一 2+1,取较短组的当前账号
    #[test]
    fn select_current_uin_picks_shorter_when_two_plus_one() {
        let two = vec![acct("100"), acct("200")];
        let one = vec![acct("300")];
        assert_eq!(select_current_uin(&two, &one).as_deref(), Some("300"));
        // 顺序无关
        assert_eq!(select_current_uin(&one, &two).as_deref(), Some("300"));
    }

    /// 双发包对齐:情况二 1+1,两次都是当前账号
    #[test]
    fn select_current_uin_picks_when_one_plus_one() {
        let a = vec![acct("572381217")];
        let b = vec![acct("572381217")];
        assert_eq!(select_current_uin(&a, &b).as_deref(), Some("572381217"));
    }

    /// 双发包对齐:情况三 2+0(当前账号非前两个登录账号),放弃转兜底
    #[test]
    fn select_current_uin_gives_up_when_two_plus_zero() {
        let two = vec![acct("100"), acct("200")];
        let zero: Vec<PtloginAccount> = vec![];
        assert!(select_current_uin(&two, &zero).is_none());
        assert!(select_current_uin(&zero, &two).is_none());
    }

    /// 双发包对齐:都返回 2 条(异常轮换),无法锁定,放弃
    #[test]
    fn select_current_uin_gives_up_when_both_two() {
        let a = vec![acct("100"), acct("200")];
        let b = vec![acct("100"), acct("200")];
        assert!(select_current_uin(&a, &b).is_none());
    }

    /// 较短组虽为 1 条但 uin 为空 → 不返回空串,放弃
    #[test]
    fn select_current_uin_rejects_empty_uin() {
        let empty = vec![acct("")];
        let two = vec![acct("100"), acct("200")];
        assert!(select_current_uin(&empty, &two).is_none());
    }
}
