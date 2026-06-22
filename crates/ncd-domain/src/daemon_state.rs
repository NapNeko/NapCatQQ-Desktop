// SnowLuma daemon 状态机 + 登录态 数据类型
//
// 纯 serde + ts-rs,零运行时依赖。行为逻辑留在 ncd-runtime/snowluma/。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// DaemonState

/// SnowLuma daemon 5 档状态机
/// - Stopped: 未启动 / 已正常退出 / 启动失败回滚后的稳态
/// - Starting: 首启 caller 正在驱动 render_globals -> spawn node.exe ->
///   wait_ready -> login,并发 caller 等 ready_notify
/// - Ready: node.exe 起好 + WebUI 就绪 + 已登录,ensure_running 返回
///   Arc<dyn SnowLumaWebUiClient>
/// - Stopping: shutdown 显式调用中,正在 logout + kill node child
/// - Crashed: node.exe 意外退出,ensure_running 直接返回 Crashed 错误,
///   依赖此 daemon 的所有 SL flavor actor 由 BotManager::run_snowluma_listener
///   级联转 Crashed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum DaemonState {
    Stopped,
    Starting,
    Ready,
    Stopping,
    Crashed,
}

// SnowLumaLoginState

/// SnowLuma 单个 Bot 在 status poller 视角下合成出来的登录状态
/// 4 档语义:
/// - Starting: QQ 进程已起,processes 还未出现自身候选 PID 的条目(注入未生效)
/// - WaitingForQrScan: processes 命中且 status == Loaded,等待用户扫码
/// - LoggedIn: processes 命中且 status == Online,OneBot pipe 已连
/// - Disconnected: processes 命中但 status 属于 {Disconnected, Error}
///   或 dispose / 连续探测失败兜底
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum SnowLumaLoginState {
    Starting,
    WaitingForQrScan,
    LoggedIn,
    Disconnected,
}
