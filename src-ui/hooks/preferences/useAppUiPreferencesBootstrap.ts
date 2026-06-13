// 启动早期从 app-settings.json 恢复 UI 偏好（首屏 Splash 之前调用）。

import { useEffect } from 'react';
import { settingsService, clientPrefsFromBackend } from '../../core/services/settings.service';
import { applySideEffects, preferencesStore } from './preferencesStore';
import { infoBarDismissPrefsStore } from './infoBarDismissPrefsStore';

let hydratedFromDisk = false;

/** 幂等：Splash 前拉磁盘设置并 applySnapshot（含 data-theme / 圆角 / localStorage）。 */
export async function hydrateAppUiPreferencesFromDisk(): Promise<void> {
    if (hydratedFromDisk) return;
    try {
        const backend = await settingsService.get();
        preferencesStore.applySnapshot(clientPrefsFromBackend(backend));
        infoBarDismissPrefsStore.applyFromUiPreferences(backend.uiPreferences);
        hydratedFromDisk = true;
    } catch {
        applySideEffects();
    }
}

export function useAppUiPreferencesBootstrap(): void {
    useEffect(() => {
        void hydrateAppUiPreferencesFromDisk();
    }, []);
}