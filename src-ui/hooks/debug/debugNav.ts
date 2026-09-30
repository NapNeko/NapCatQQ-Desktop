// 从别的页面（Bot 卡片、右键菜单）跳进调试台并选中某个 Bot 的桥。
//
// 路由壳在 AppNext 里，hooks 层不能反向 import 它，所以壳启动时把「跳转」注册进来，
// 入口只管 openDebugConsole。目标 Bot 先记成「待选」，再触发跳转：
// 调试台页面还没挂载时，挂载后自己 consume；已经挂着（用户在调试台里、又从别处触发）时，
// 靠 subscribePendingDebugBot 的订阅响应变化。

import { useSyncExternalStore } from 'react';

let navigator: (() => void) | null = null;
let pendingBot: string | null = null;
const listeners = new Set<() => void>();

function emit(): void {
    for (const l of listeners) l();
}

/** 路由壳注册；返回的注销函数只会撤掉自己注册的那份（StrictMode 下重复注册不会互相踩） */
export function registerDebugNavigator(fn: () => void): () => void {
    navigator = fn;
    return () => {
        if (navigator === fn) navigator = null;
    };
}

/** 不传 botId 就只是打开调试台；上次没被取走的待选 Bot 也一并清掉，免得选到过期的目标 */
export function openDebugConsole(botId?: string): void {
    const next = botId ?? null;
    if (next !== pendingBot) {
        pendingBot = next;
        emit();
    }
    navigator?.();
}

/** 取走待选 Bot；取过一次就清空 */
export function consumePendingDebugBot(): string | null {
    const bot = pendingBot;
    if (bot !== null) {
        pendingBot = null;
        emit();
    }
    return bot;
}

export function subscribePendingDebugBot(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

export function getPendingDebugBot(): string | null {
    return pendingBot;
}

/** 已经挂着的调试台页面用：待选 Bot 一变就重渲染，effect 里再 consume */
export function usePendingDebugBot(): string | null {
    return useSyncExternalStore(subscribePendingDebugBot, getPendingDebugBot, getPendingDebugBot);
}

/** 测试用 */
export function _resetDebugNavForTests(): void {
    navigator = null;
    pendingBot = null;
    listeners.clear();
}
