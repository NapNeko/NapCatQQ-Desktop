// InfoBar 自动关闭偏好（来自 app-settings.uiPreferences，保存设置后更新）。

import { useSyncExternalStore } from 'react';
import {
    DEFAULT_INFOBAR_DISMISS,
    infoBarDismissFromUiPreferences,
    type InfoBarDismissPrefs,
} from '../../core/domain/ui/infoBarDismiss';
import type { AppUiPreferences } from '../../core/ipc/generated/domain/AppUiPreferences';

let prefs: InfoBarDismissPrefs = { ...DEFAULT_INFOBAR_DISMISS };
const listeners = new Set<() => void>();

function notify() {
    for (const fn of listeners) fn();
}

export const infoBarDismissPrefsStore = {
    getSnapshot(): InfoBarDismissPrefs {
        return prefs;
    },
    subscribe(listener: () => void): () => void {
        listeners.add(listener);
        return () => listeners.delete(listener);
    },
    applyFromUiPreferences(ui: AppUiPreferences): void {
        prefs = infoBarDismissFromUiPreferences(ui);
        notify();
    },
    /** 测试用 */
    _reset(): void {
        prefs = { ...DEFAULT_INFOBAR_DISMISS };
        notify();
    },
};

export function useInfoBarDismissPrefs(): InfoBarDismissPrefs {
    return useSyncExternalStore(
        infoBarDismissPrefsStore.subscribe,
        infoBarDismissPrefsStore.getSnapshot,
        infoBarDismissPrefsStore.getSnapshot,
    );
}