// 只保存窗口布局偏好，不持久化消息或草稿。
import { useSyncExternalStore } from 'react';

interface ChatPreferences { listWidth: number; composerHeight: number | null; hiddenConversations: Record<string, string[]> }
const STORAGE_KEY = 'ncd.chat.ui.v1';
const defaults: ChatPreferences = { listWidth: 260, composerHeight: null, hiddenConversations: {} };
function read(): ChatPreferences {
    try {
        const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
        if (!value || typeof value !== 'object') return defaults;
        const data = value as Record<string, unknown>;
        return {
            listWidth: typeof data.listWidth === 'number' && Number.isFinite(data.listWidth) ? Math.min(380, Math.max(220, data.listWidth)) : 260,
            composerHeight: typeof data.composerHeight === 'number' && Number.isFinite(data.composerHeight) ? Math.min(420, Math.max(52, data.composerHeight)) : null,
            hiddenConversations: data.hiddenConversations && typeof data.hiddenConversations === 'object' && !Array.isArray(data.hiddenConversations)
                ? Object.fromEntries(Object.entries(data.hiddenConversations).filter(([, keys]) => Array.isArray(keys)).map(([account, keys]) => [account, (keys as unknown[]).filter((key): key is string => typeof key === 'string')])) : {},
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

export function setConversationHidden(account: string, key: string, hidden: boolean) {
    const keys = new Set(preferences.hiddenConversations[account] ?? []);
    if (keys.has(key) === hidden) return;
    if (hidden) keys.add(key); else keys.delete(key);
    setChatPreferences({ hiddenConversations: { ...preferences.hiddenConversations, [account]: [...keys] } });
}
