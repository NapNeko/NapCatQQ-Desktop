// 设置页统一草稿：后端 app-settings + 仅客户端偏好，全部经右上角「保存设置」落盘。

import type { MotionLevel } from '../../core/design/motion';
import { scaleDuration } from '../../core/design/motion';
import type { RadiusStyle } from '../../core/design/radius';
import { clampPerformanceMonitorIntervalMs } from '../../core/domain/performance/performanceSettings';
import {
    clientPrefsFromBackend,
    type BackendSettings,
} from '../../core/services/settings.service';
import {
    appPreferencesToAppUiPreferences,
    infoBarDismissPrefsFromDraftFields,
} from '../../core/domain/settings/ui-preferences-bridge';
import {
    infoBarDismissDraftFromStored,
    infoBarDismissFromUiPreferences,
    type InfoBarDismissDraftSlice,
} from '../../core/domain/ui/infoBarDismiss';
import {
    preferencesStore,
    type AppPreferences,
    type ThemeMode,
    normalizeCloseAction,
} from '../../hooks/preferences/preferencesStore';
import { infoBarDismissPrefsStore } from '../../hooks/preferences/infoBarDismissPrefsStore';
import {
    taskQueueCleanupDraftFromStored,
    type TaskQueueCleanupDraftSlice,
} from '../../core/domain/task-queue/cleanup';
import { taskQueueCleanupPrefsStore } from '../../hooks/task-queue/taskQueueCleanupPrefsStore';
import { playThemeTransition } from '../../core/design/themeTransition';

/** 设置页可编辑项的完整草稿（通用 / 网络 Tab + 本机偏好）。 */
export type SettingsDraft = Omit<BackendSettings, 'taskQueueCleanup'> & {
    theme: ThemeMode;
    showMascot: boolean;
    motionEnabled: boolean;
    motionLevel: MotionLevel;
    motionSpeed: number;
    radiusStyle: RadiusStyle;
} & InfoBarDismissDraftSlice &
    TaskQueueCleanupDraftSlice;

function infoBarDismissDraftFromSettingsDraft(
    draft: SettingsDraft,
): InfoBarDismissDraftSlice {
    return {
        infoBarDismissInfoEnabled: draft.infoBarDismissInfoEnabled,
        infoBarDismissInfoMs: draft.infoBarDismissInfoMs,
        infoBarDismissSuccessEnabled: draft.infoBarDismissSuccessEnabled,
        infoBarDismissSuccessMs: draft.infoBarDismissSuccessMs,
        infoBarDismissWarningEnabled: draft.infoBarDismissWarningEnabled,
        infoBarDismissWarningMs: draft.infoBarDismissWarningMs,
    };
}

export function draftFromBackendAndPrefs(
    backend: BackendSettings,
    _prefs?: AppPreferences,
): SettingsDraft {
    const client = clientPrefsFromBackend(backend);
    const dismiss = infoBarDismissFromUiPreferences(backend.uiPreferences);
    const ibDraft = infoBarDismissDraftFromStored(dismiss);
    const tqDraft = taskQueueCleanupDraftFromStored(backend.taskQueueCleanup);
    return {
        botLoginCheckIntervalMs: backend.botLoginCheckIntervalMs,
        botOfflineWebHookNotice: backend.botOfflineWebHookNotice,
        botOfflineEmailNotice: backend.botOfflineEmailNotice,
        performanceMonitorEnabled: backend.performanceMonitorEnabled,
        performanceMonitorIntervalMs: backend.performanceMonitorIntervalMs,
        githubPat: backend.githubPat,
        closeAction: backend.closeAction,
        afterCloseUiBehavior: backend.afterCloseUiBehavior,
        enterLightweightDelaySecs: backend.enterLightweightDelaySecs,
        uiModeOnStartup: backend.uiModeOnStartup,
        minimizeToTrayCountsAsHidden: backend.minimizeToTrayCountsAsHidden,
        notifyOnOffline: backend.notifyOnOffline,
        notifyOnBotCrashed: backend.notifyOnBotCrashed,
        notifyOnLoginKicked: backend.notifyOnLoginKicked,
        uiPreferences: backend.uiPreferences,
        // P1 主动探活
        remoteHostHealthProbeEnabled: backend.remoteHostHealthProbeEnabled,
        remoteHostHealthProbeIntervalMs: backend.remoteHostHealthProbeIntervalMs,
        theme: client.theme,
        showMascot: client.showMascot,
        motionEnabled: client.motionEnabled,
        motionLevel: client.motionLevel,
        motionSpeed: client.motionSpeed,
        radiusStyle: client.radiusStyle,
        ...ibDraft,
        ...tqDraft,
    };
}

export function backendSlice(draft: SettingsDraft): BackendSettings {
    const dismiss = infoBarDismissPrefsFromDraftFields(
        infoBarDismissDraftFromSettingsDraft(draft),
    );
    return {
        botLoginCheckIntervalMs: draft.botLoginCheckIntervalMs,
        botOfflineWebHookNotice: draft.botOfflineWebHookNotice,
        botOfflineEmailNotice: draft.botOfflineEmailNotice,
        performanceMonitorEnabled: draft.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            draft.performanceMonitorIntervalMs,
        ),
        taskQueueCleanup: {
            taskQueueCleanupEnabled: draft.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: draft.taskQueueCleanupLingerMs,
        },
        githubPat: draft.githubPat,
        closeAction: draft.closeAction,
        afterCloseUiBehavior: draft.afterCloseUiBehavior,
        enterLightweightDelaySecs: draft.enterLightweightDelaySecs,
        uiModeOnStartup: draft.uiModeOnStartup,
        minimizeToTrayCountsAsHidden: draft.minimizeToTrayCountsAsHidden,
        notifyOnOffline: draft.notifyOnOffline,
        notifyOnBotCrashed: draft.notifyOnBotCrashed,
        notifyOnLoginKicked: draft.notifyOnLoginKicked,
        uiPreferences: appPreferencesToAppUiPreferences(
            {
                theme: draft.theme,
                showMascot: draft.showMascot,
                closeAction: draft.closeAction,
                motionEnabled: draft.motionEnabled,
                motionLevel: draft.motionLevel,
                motionSpeed: draft.motionSpeed,
                radiusStyle: draft.radiusStyle,
            },
            dismiss,
        ),
        // P1 主动探活
        remoteHostHealthProbeEnabled: draft.remoteHostHealthProbeEnabled,
        remoteHostHealthProbeIntervalMs: draft.remoteHostHealthProbeIntervalMs,
    };
}

export function isSettingsDirty(
    draft: SettingsDraft,
    backend: BackendSettings,
): boolean {
    const baseline = draftFromBackendAndPrefs(backend);
    return (
        draft.botLoginCheckIntervalMs !== baseline.botLoginCheckIntervalMs ||
        draft.botOfflineWebHookNotice !== baseline.botOfflineWebHookNotice ||
        draft.botOfflineEmailNotice !== baseline.botOfflineEmailNotice ||
        draft.performanceMonitorEnabled !== baseline.performanceMonitorEnabled ||
        draft.performanceMonitorIntervalMs !== baseline.performanceMonitorIntervalMs ||
        draft.githubPat !== baseline.githubPat ||
        draft.closeAction !== baseline.closeAction ||
        draft.afterCloseUiBehavior !== baseline.afterCloseUiBehavior ||
        draft.enterLightweightDelaySecs !== baseline.enterLightweightDelaySecs ||
        draft.uiModeOnStartup !== baseline.uiModeOnStartup ||
        draft.minimizeToTrayCountsAsHidden !==
            baseline.minimizeToTrayCountsAsHidden ||
        draft.notifyOnOffline !== baseline.notifyOnOffline ||
        draft.notifyOnBotCrashed !== baseline.notifyOnBotCrashed ||
        draft.notifyOnLoginKicked !== baseline.notifyOnLoginKicked ||
        draft.theme !== baseline.theme ||
        draft.showMascot !== baseline.showMascot ||
        draft.motionEnabled !== baseline.motionEnabled ||
        draft.motionLevel !== baseline.motionLevel ||
        draft.motionSpeed !== baseline.motionSpeed ||
        draft.radiusStyle !== baseline.radiusStyle ||
        draft.infoBarDismissInfoEnabled !== baseline.infoBarDismissInfoEnabled ||
        draft.infoBarDismissInfoMs !== baseline.infoBarDismissInfoMs ||
        draft.infoBarDismissSuccessEnabled !== baseline.infoBarDismissSuccessEnabled ||
        draft.infoBarDismissSuccessMs !== baseline.infoBarDismissSuccessMs ||
        draft.infoBarDismissWarningEnabled !== baseline.infoBarDismissWarningEnabled ||
        draft.infoBarDismissWarningMs !== baseline.infoBarDismissWarningMs ||
        draft.taskQueueCleanupEnabled !== baseline.taskQueueCleanupEnabled ||
        draft.taskQueueCleanupLingerMs !== baseline.taskQueueCleanupLingerMs ||
        // P1 主动探活
        draft.remoteHostHealthProbeEnabled !== baseline.remoteHostHealthProbeEnabled ||
        draft.remoteHostHealthProbeIntervalMs !== baseline.remoteHostHealthProbeIntervalMs
    );
}

export async function applyClientPrefsFromDraft(draft: SettingsDraft): Promise<void> {
    const oldTheme = preferencesStore.get().theme;
    const themeChanged = draft.theme !== oldTheme;

    if (themeChanged) {
        const enabled = draft.motionEnabled;
        const level = draft.motionLevel;
        const speed = draft.motionSpeed;

        let duration: number;
        let easing: string;
        if (!enabled) {
            duration = 0;
            easing = 'linear';
        } else if (level === 'elegant') {
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
    infoBarDismissPrefsStore.applyFromUiPreferences(
        appPreferencesToAppUiPreferences(
            {
                theme: draft.theme,
                showMascot: draft.showMascot,
                closeAction: draft.closeAction,
                motionEnabled: draft.motionEnabled,
                motionLevel: draft.motionLevel,
                motionSpeed: draft.motionSpeed,
                radiusStyle: draft.radiusStyle,
            },
            infoBarDismissPrefsFromDraftFields(
                infoBarDismissDraftFromSettingsDraft(draft),
            ),
        ),
    );
    taskQueueCleanupPrefsStore.applyPrefs({
        taskQueueCleanupEnabled: draft.taskQueueCleanupEnabled,
        taskQueueCleanupLingerMs: draft.taskQueueCleanupLingerMs,
    });
}

export { normalizeCloseAction };
