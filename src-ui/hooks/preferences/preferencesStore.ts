// 客户端偏好的模块级 store。纯前端，落盘到 localStorage。
//
// 跟 napcatLoginStore / globalInfoBarStore 同一套：模块级 state + listeners +
// useSyncExternalStore 视图。理由见 FRONTEND_HANDOFF §3.8 教训 1：任何"长期
// 累积、跨路由保留"的 state 都必须模块级，不能 useReducer 组件级。
//
// 当前承载：
//   theme           light / dark / auto
//   showMascot      true / false
//   closeAction     close（关闭程序）/ tray（最小化到托盘，留给 Tauri 后续接入）
//   windowOpacity   80-100 整数百分比
//
// 这些字段都不依赖后端 IPC，纯客户端偏好。后端可控的偏好（轮询间隔 /
// GitHub PAT / 邮件 webhook）等扩了 IPC 再开新 store，不混进来。

import { useSyncExternalStore } from 'react';

export type ThemeMode = 'light' | 'dark' | 'auto';
export type CloseAction = 'close' | 'tray';

export interface AppPreferences {
    theme: ThemeMode;
    showMascot: boolean;
    closeAction: CloseAction;
    /** 窗口不透明度，整数百分比（80-100），<100 时窗口背景半透明。 */
    windowOpacity: number;
}

const STORAGE_KEY = 'ncd:preferences:v1';

const defaultPrefs: AppPreferences = {
    theme: 'auto',
    showMascot: true,
    closeAction: 'close',
    windowOpacity: 100,
};

let state: AppPreferences = loadFromStorage();
const listeners = new Set<() => void>();

function loadFromStorage(): AppPreferences {
    if (typeof window === 'undefined') return defaultPrefs;
    try {
        const raw = window.localStorage.getItem(STORAGE_KEY);
        if (!raw) return defaultPrefs;
        const parsed = JSON.parse(raw) as Partial<AppPreferences>;
        return {
            theme: normalizeTheme(parsed.theme),
            showMascot: parsed.showMascot !== false,
            closeAction: parsed.closeAction === 'tray' ? 'tray' : 'close',
            windowOpacity: clampOpacity(parsed.windowOpacity),
        };
    } catch {
        return defaultPrefs;
    }
}

function persist() {
    if (typeof window === 'undefined') return;
    try {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
        // localStorage 满了 / 隐私模式；偏好丢就丢，不阻塞业务。
    }
}

function normalizeTheme(raw: unknown): ThemeMode {
    return raw === 'light' || raw === 'dark' || raw === 'auto' ? raw : 'auto';
}

function clampOpacity(raw: unknown): number {
    const n = typeof raw === 'number' ? raw : 100;
    if (Number.isNaN(n)) return 100;
    return Math.max(80, Math.min(100, Math.round(n)));
}

function notify() {
    for (const fn of listeners) fn();
}

function update(patch: Partial<AppPreferences>) {
    state = { ...state, ...patch };
    persist();
    notify();
    applySideEffects();
}

/// 把当前偏好应用到 DOM / window。`AppNext` 启动时调一次让初始状态生效；
/// 用户切偏好时由 update 自动调。
export function applySideEffects() {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    // 主题：只设 light/dark；auto 时 attribute 留空让 CSS 走 prefers-color-scheme。
    if (state.theme === 'auto') {
        root.removeAttribute('data-theme');
    } else {
        root.setAttribute('data-theme', state.theme);
    }
    // 窗口不透明度：通过 root style 的 background-color alpha 实现简版。
    // Tauri 真窗口透明需要 transparent: true 并改 window.opacity，留给后续。
    root.style.setProperty('--window-opacity', String(state.windowOpacity / 100));
}

export const preferencesStore = {
    get(): AppPreferences {
        return state;
    },
    setTheme(theme: ThemeMode) {
        update({ theme: normalizeTheme(theme) });
    },
    setShowMascot(show: boolean) {
        update({ showMascot: !!show });
    },
    setCloseAction(action: CloseAction) {
        update({ closeAction: action === 'tray' ? 'tray' : 'close' });
    },
    setWindowOpacity(opacity: number) {
        update({ windowOpacity: clampOpacity(opacity) });
    },
    reset() {
        state = { ...defaultPrefs };
        persist();
        notify();
        applySideEffects();
    },
    subscribe(listener: () => void): () => void {
        listeners.add(listener);
        return () => {
            listeners.delete(listener);
        };
    },
};

/// React 视图。组件 mount 时同步 store 当前快照，store 变化时重新渲染。
export function usePreferences(): AppPreferences {
    return useSyncExternalStore(
        (l) => preferencesStore.subscribe(l),
        () => preferencesStore.get(),
        () => state,
    );
}
