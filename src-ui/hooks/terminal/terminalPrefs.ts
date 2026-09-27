// 终端偏好（字号、光标、右键行为、关键字高亮…）和面板布局（高度、文件栏）。
// 纯前端的东西，存 localStorage，改了立刻生效，不走设置页的「保存设置」。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import type { LocalShellKind } from '../../core/ipc/generated/domain/LocalShellKind';
import type { TerminalSnippet } from '../../core/ipc/generated/domain/TerminalSnippet';

export type TerminalCursorStyle = 'block' | 'bar' | 'underline';
export type TerminalRightClick = 'menu' | 'paste';
export type TerminalColorScheme = 'auto' | 'dark';

export interface TerminalPrefs {
    fontSize: number;
    lineHeight: number;
    cursorStyle: TerminalCursorStyle;
    cursorBlink: boolean;
    scrollback: number;
    copyOnSelect: boolean;
    rightClick: TerminalRightClick;
    highlight: boolean;
    confirmMultilinePaste: boolean;
    defaultShell: LocalShellKind | null;
    gpu: boolean;
    colorScheme: TerminalColorScheme;
    /** 用户自己加的常用命令 */
    snippets: TerminalSnippet[];
}

export interface TerminalLayoutPrefs {
    /** 面板高度（像素） */
    height: number;
    filesOpen: boolean;
    filesWidth: number;
}

export const DEFAULT_TERMINAL_PREFS: TerminalPrefs = {
    fontSize: 13,
    lineHeight: 1.15,
    cursorStyle: 'bar',
    cursorBlink: true,
    scrollback: 5000,
    copyOnSelect: false,
    rightClick: 'menu',
    highlight: true,
    confirmMultilinePaste: true,
    defaultShell: null,
    gpu: true,
    colorScheme: 'auto',
    snippets: [],
};

export const DEFAULT_TERMINAL_LAYOUT: TerminalLayoutPrefs = {
    height: 320,
    filesOpen: false,
    filesWidth: 280,
};

export const FONT_SIZE_RANGE = { min: 9, max: 28 } as const;
export const SCROLLBACK_RANGE = { min: 500, max: 100_000 } as const;
export const DOCK_HEIGHT_MIN = 160;

const PREFS_KEY = 'ncd.terminal.prefs.v1';
const LAYOUT_KEY = 'ncd.terminal.layout.v1';

function load<T extends object>(key: string, defaults: T): T {
    try {
        const raw = window.localStorage.getItem(key);
        if (!raw) return defaults;
        const parsed = JSON.parse(raw) as Partial<T>;
        return { ...defaults, ...parsed };
    } catch {
        return defaults;
    }
}

function save(key: string, value: unknown) {
    try {
        window.localStorage.setItem(key, JSON.stringify(value));
    } catch {
        // 存不了（隐私模式、配额满）就只在这次会话里生效
    }
}

function clampPrefs(p: TerminalPrefs): TerminalPrefs {
    return {
        ...p,
        fontSize: Math.round(Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, p.fontSize))),
        lineHeight: Math.min(2, Math.max(1, p.lineHeight)),
        scrollback: Math.round(Math.min(SCROLLBACK_RANGE.max, Math.max(SCROLLBACK_RANGE.min, p.scrollback))),
        snippets: Array.isArray(p.snippets) ? p.snippets.filter((s) => s && s.command) : [],
    };
}

const prefsStore = createStore<TerminalPrefs>(clampPrefs(load(PREFS_KEY, DEFAULT_TERMINAL_PREFS)));
const layoutStore = createStore<TerminalLayoutPrefs>(load(LAYOUT_KEY, DEFAULT_TERMINAL_LAYOUT));

export const terminalPrefs = {
    get: prefsStore.getSnapshot,
    subscribe: prefsStore.subscribe,
    patch(patch: Partial<TerminalPrefs>) {
        const next = clampPrefs({ ...prefsStore.getSnapshot(), ...patch });
        prefsStore.setState(next);
        save(PREFS_KEY, next);
    },
    reset() {
        prefsStore.setState(DEFAULT_TERMINAL_PREFS);
        save(PREFS_KEY, DEFAULT_TERMINAL_PREFS);
    },
    zoom(delta: number) {
        const current = prefsStore.getSnapshot().fontSize;
        terminalPrefs.patch({ fontSize: delta === 0 ? DEFAULT_TERMINAL_PREFS.fontSize : current + delta });
    },
};

export const terminalLayout = {
    get: layoutStore.getSnapshot,
    subscribe: layoutStore.subscribe,
    patch(patch: Partial<TerminalLayoutPrefs>) {
        const next = { ...layoutStore.getSnapshot(), ...patch };
        layoutStore.setState(next);
        save(LAYOUT_KEY, next);
    },
};

export function useTerminalPrefs(): TerminalPrefs {
    return useSyncExternalStore(prefsStore.subscribe, prefsStore.getSnapshot, prefsStore.getSnapshot);
}

export function useTerminalLayout(): TerminalLayoutPrefs {
    return useSyncExternalStore(layoutStore.subscribe, layoutStore.getSnapshot, layoutStore.getSnapshot);
}
