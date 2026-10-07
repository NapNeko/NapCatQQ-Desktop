// 调试台弹出窗（独立工具窗）IPC + 本 webview 是不是弹出窗的标识。

import { invoke, isTauri } from '../ipc/transport';

// label 单一来源在 domain；保留原导出面，调用方不改
export { DEBUG_WINDOW_LABEL } from '../domain/windows';

// 弹出窗和主窗不共享 JS 世界，「我是谁」由 main.tsx 启动时按窗口 label 置一次，同步读
let isPopout = false;
export function markDebugPopoutWindow(): void {
    isPopout = true;
}
/** 本 webview 是不是调试台弹出窗；TopBar 据此把「弹出」按钮藏掉 */
export function isDebugPopoutWindow(): boolean {
    return isPopout;
}

export const debugWindowService = {
    /** 创建或聚焦弹出窗（label 固定，重复调只聚焦）。浏览器预览无窗可弹 */
    open: (): Promise<void> => (isTauri ? invoke('open_debug_window') : Promise.resolve()),

    /** 弹出窗前端画好首帧后叫它显示（对齐主窗 reveal 的启动时序） */
    reveal: (): Promise<void> => (isTauri ? invoke('reveal_debug_window') : Promise.resolve()),

    /** 弹出窗开着就聚焦并回 true；主窗的调试台入口据此让位给它 */
    focusIfOpen: (): Promise<boolean> =>
        isTauri ? invoke<boolean>('focus_debug_window') : Promise.resolve(false),
};
