// 只保存界面偏好，不持久化账号、消息或草稿。
import { useSyncExternalStore } from 'react';

export type SendShortcut = 'enter' | 'ctrl-enter';
interface ChatPreferences { listWidth: number; sendShortcut: SendShortcut }
const STORAGE_KEY = 'ncd.chat.ui.v1';
const defaults: ChatPreferences = { listWidth: 260, sendShortcut: 'enter' };
function read(): ChatPreferences {
    try {
        const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
        if (!value || typeof value !== 'object') return defaults;
        const data = value as Record<string, unknown>;
        return {
            listWidth: typeof data.listWidth === 'number' && Number.isFinite(data.listWidth) ? Math.min(380, Math.max(220, data.listWidth)) : 260,
            sendShortcut: data.sendShortcut === 'ctrl-enter' ? 'ctrl-enter' : 'enter',
        };
    } catch { return defaults; }
}
let preferences = read();
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const snapshot = () => preferences;
export function useChatPreferences() { return useSyncExternalStore(subscribe, snapshot, snapshot); }
export function setChatPreferences(patch: Partial<ChatPreferences>) {
    preferences = { ...preferences, ...patch };
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences)); } catch { /* 禁用存储时仍保留本次会话偏好。 */ }
    for (const listener of listeners) listener();
}
