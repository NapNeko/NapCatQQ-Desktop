// 设置页统一草稿：后端 app-settings + 仅客户端偏好，全部经右上角「保存设置」落盘。

import type { MotionLevel } from '../../core/design/motion';
import { scaleDuration } from '../../core/design/motion';
import type { RadiusStyle } from '../../core/design/radius';
import { clampPerformanceMonitorIntervalMs } from '../../core/domain/performance/performanceSettings';
import type { BackendSettings } from '../../core/services/settings.service';
import {
    preferencesStore,
    type AppPreferences,
    type ThemeMode,
    normalizeCloseAction,
} from '../../hooks/preferences/preferencesStore';
import { playThemeTransition } from '../../core/design/themeTransition';

/** 设置页可编辑项的完整草稿（通用 / 网络 Tab + 本机偏好）。 */
export type SettingsDraft = BackendSettings & {
    theme: ThemeMode;
    showMascot: boolean;
    motionEnabled: boolean;
    motionLevel: MotionLevel;
    motionSpeed: number;
    radiusStyle: RadiusStyle;
};

export function draftFromBackendAndPrefs(
    backend: BackendSettings,
    prefs: AppPreferences,
): SettingsDraft {
    return {
        ...backend,
        theme: prefs.theme,
        showMascot: prefs.showMascot,
        motionEnabled: prefs.motionEnabled,
        motionLevel: prefs.motionLevel,
        motionSpeed: prefs.motionSpeed,
        radiusStyle: prefs.radiusStyle,
    };
}

export function backendSlice(draft: SettingsDraft): BackendSettings {
    return {
        botLoginCheckIntervalMs: draft.botLoginCheckIntervalMs,
        performanceMonitorEnabled: draft.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            draft.performanceMonitorIntervalMs,
        ),
        githubPat: draft.githubPat,
        closeAction: draft.closeAction,
    };
}

export function isSettingsDirty(
    draft: SettingsDraft,
    backend: BackendSettings,
    prefs: AppPreferences,
): boolean {
    const baseline = draftFromBackendAndPrefs(backend, prefs);
    return (
        draft.botLoginCheckIntervalMs !== baseline.botLoginCheckIntervalMs ||
        draft.performanceMonitorEnabled !== baseline.performanceMonitorEnabled ||
        draft.performanceMonitorIntervalMs !== baseline.performanceMonitorIntervalMs ||
        draft.githubPat !== baseline.githubPat ||
        draft.closeAction !== baseline.closeAction ||
        draft.theme !== baseline.theme ||
        draft.showMascot !== baseline.showMascot ||
        draft.motionEnabled !== baseline.motionEnabled ||
        draft.motionLevel !== baseline.motionLevel ||
        draft.motionSpeed !== baseline.motionSpeed ||
        draft.radiusStyle !== baseline.radiusStyle
    );
}

export async function applyClientPrefsFromDraft(draft: SettingsDraft): Promise<void> {
    const oldTheme = preferencesStore.get().theme;
    const themeChanged = draft.theme !== oldTheme;

    if (themeChanged) {
        // 读取 motion 设置（用 draft 值，用户刚保存的就是他们想要的）
        const enabled = draft.motionEnabled;
        const level = draft.motionLevel;
        const speed = draft.motionSpeed;

        let duration: number;
        let easing: string;
        if (!enabled) {
            duration = 0;
            easing = 'linear';
        } else if (level === 'elegant') {
            // elegant 档不播放动画
            duration = 0;
            easing = 'ease-in-out';
        } else if (level === 'standard') {
            duration = scaleDuration(1.8, speed);
            easing = 'ease-in-out';
        } else {
            duration = scaleDuration(2.8, speed);
            easing = 'ease-in-out';
        }

        await playThemeTransition(
            () => {
                preferencesStore.applySnapshot({
                    theme: draft.theme,
                    showMascot: draft.showMascot,
                    closeAction: draft.closeAction,
                    motionEnabled: draft.motionEnabled,
                    motionLevel: draft.motionLevel,
                    motionSpeed: draft.motionSpeed,
                    radiusStyle: draft.radiusStyle,
                });
            },
            { enabled, level, duration, easing },
        );
    } else {
        preferencesStore.applySnapshot({
            theme: draft.theme,
            showMascot: draft.showMascot,
            closeAction: draft.closeAction,
            motionEnabled: draft.motionEnabled,
            motionLevel: draft.motionLevel,
            motionSpeed: draft.motionSpeed,
            radiusStyle: draft.radiusStyle,
        });
    }
}

export { normalizeCloseAction };