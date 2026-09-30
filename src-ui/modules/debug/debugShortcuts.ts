// 调试台快捷键：挂在 window 上，页面挂着时生效。
//
// 规矩和应用端保存栏的 Ctrl+S 一样：焦点在对话框（含弹出层，Radix 的 Popover 也是 role="dialog"）
// 或终端里时不抢；别的组件已经处理过的键（defaultPrevented，比如 JSON 编辑器自己的 Ctrl+Enter、
// 补全框的 Esc）不重复处理；输入法组字时不处理。永远不碰 Ctrl+`，那是终端的开关。
//
// 多处可以各自调用：页面管标签页和命令面板，中栏管发送 / 取消，各管各的键互不相扰。
// 处理函数返回 false 表示「这次不归我」（比如没有进行中的调用时按 Esc），这时不拦默认行为，
// Esc 还能照常关掉别的东西。

import { useEffect, useRef, type RefObject } from 'react';

export type DebugShortcut = 'palette' | 'send' | 'cancel' | 'closeTab' | 'nextTab' | 'prevTab' | 'reopenTab';

export type DebugShortcutHandlers = Partial<Record<DebugShortcut, () => boolean | void>>;

type KeyLike = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>;

/** 按键 → 快捷键；不是调试台快捷键返回 null。只看按键本身，不管焦点在哪 */
export function matchDebugShortcut(e: KeyLike): DebugShortcut | null {
    if (e.altKey) return null;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

    if (key === 'Escape') return mod || e.shiftKey ? null : 'cancel';
    // Ctrl+Tab 在 macOS 上也是 Ctrl，不是 ⌘
    if (key === 'Tab') return e.ctrlKey && !e.metaKey ? (e.shiftKey ? 'prevTab' : 'nextTab') : null;
    if (!mod) return null;
    if (key === 'Enter') return e.shiftKey ? null : 'send';
    if (key === 'k') return e.shiftKey ? null : 'palette';
    if (key === 'w') return e.shiftKey ? null : 'closeTab';
    if (key === 't') return e.shiftKey ? 'reopenTab' : null;
    return null;
}

function insideIgnoredArea(target: EventTarget | null): boolean {
    return target instanceof Element && !!target.closest('[role="dialog"], [role="alertdialog"], .xterm');
}

/** 页面被终端整个盖住（main 加了 hidden）时，快捷键不该在看不见的页面上关标签 */
function scopeHidden(scope: RefObject<HTMLElement | null> | undefined): boolean {
    if (!scope) return false;
    const el = scope.current;
    if (!el || !el.isConnected) return true;
    // checkVisibility 只看 display / content-visibility，正好对应「被 hidden 类藏起来」；没有这个 API 的环境当作可见
    return typeof el.checkVisibility === 'function' ? !el.checkVisibility() : false;
}

/** 按住不放时这些只该触发一次：连发请求、一口气关光标签都不是用户想要的 */
const NO_REPEAT: ReadonlySet<DebugShortcut> = new Set(['send', 'closeTab', 'reopenTab', 'palette']);

interface Options {
    /** 页面根节点；它看不见（被盖住、没挂上）时整组快捷键不生效 */
    scopeRef?: RefObject<HTMLElement | null>;
    enabled?: boolean;
}

export function useDebugShortcuts(handlers: DebugShortcutHandlers, options: Options = {}): void {
    const { scopeRef, enabled = true } = options;
    // 处理函数每次渲染都可能是新的；监听器只挂一次，调用时读最新的
    const handlersRef = useRef(handlers);
    handlersRef.current = handlers;

    useEffect(() => {
        if (!enabled) return;
        const onKeyDown = (e: KeyboardEvent) => {
            if (e.defaultPrevented || e.isComposing) return;
            const which = matchDebugShortcut(e);
            if (!which || (e.repeat && NO_REPEAT.has(which))) return;
            const handler = handlersRef.current[which];
            if (!handler) return;
            if (insideIgnoredArea(e.target) || scopeHidden(scopeRef)) return;
            if (handler() === false) return;
            e.preventDefault();
        };
        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, [enabled, scopeRef]);
}
