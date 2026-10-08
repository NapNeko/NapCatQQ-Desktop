// 独立窗口 label 与「我是哪个窗口」的纯判定，单一来源。
// 需要跨模块共享写入的运行时标识（弹出窗 mark / restoreReading 联动）留在对应 service，这里只放纯逻辑。

/** 与 Rust 侧 chat_window.rs 的 CHAT_WINDOW_LABEL 一致；主入口按它分发聊天弹出窗根组件 */
export const CHAT_WINDOW_LABEL = 'chat-panel';

/** 与 Rust 侧 commands/window.rs 的 DEBUG_WINDOW_LABEL 一致；主入口按它分发调试弹出窗根组件 */
export const DEBUG_WINDOW_LABEL = 'debug-console';

/** 浏览器预览用 URL 参数声明「这是聊天弹出窗」：没有真实窗口 label 可问，只能看查询串 */
export function isChatPopoutSearch(search: string): boolean {
    return new URLSearchParams(search).has('chatPanel');
}
