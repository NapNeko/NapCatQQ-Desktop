// 同一窗口共用刷新时钟，避免固定 URL 让 WebView 长期复用旧头像。
import { useSyncExternalStore } from 'react';

export const AVATAR_REFRESH_MS = 5 * 60_000;

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setTimeout> | undefined;

const snapshot = () => Math.floor(Date.now() / AVATAR_REFRESH_MS);
const idleSubscribe = () => () => {};

function refresh() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    if (document.visibilityState === 'hidden') return;
    for (const listener of listeners) listener();
    if (listeners.size > 0) {
        timer = setTimeout(refresh, AVATAR_REFRESH_MS - (Date.now() % AVATAR_REFRESH_MS));
    }
}

function subscribe(listener: () => void) {
    listeners.add(listener);
    if (listeners.size === 1) {
        document.addEventListener('visibilitychange', refresh);
        window.addEventListener('focus', refresh);
        refresh();
    }
    return () => {
        listeners.delete(listener);
        if (listeners.size === 0) {
            if (timer !== undefined) clearTimeout(timer);
            timer = undefined;
            document.removeEventListener('visibilitychange', refresh);
            window.removeEventListener('focus', refresh);
        }
    };
}

export function useAvatarUrl(url: string | null): string | null {
    const version = useSyncExternalStore(url ? subscribe : idleSubscribe, snapshot, snapshot);
    return url ? `${url}${url.includes('?') ? '&' : '?'}_ncdAvatar=${version}` : null;
}
