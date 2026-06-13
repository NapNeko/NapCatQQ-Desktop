// 浏览器 dev 模式下的后端设置 mock。

import type { BackendSettings } from '../../services/settings.service';
import { defaultAppUiPreferencesFromPrefs } from '../../domain/settings/ui-preferences-bridge';
import { DEFAULT_TASK_QUEUE_CLEANUP } from '../../domain/task-queue/cleanup';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';

export const mockBackendSettings: BackendSettings = {
    botLoginCheckIntervalMs: 5000,
    performanceMonitorEnabled: true,
    performanceMonitorIntervalMs: 1200,
    taskQueueCleanup: { ...DEFAULT_TASK_QUEUE_CLEANUP },
    githubPat: '',
    closeAction: 'close',
    uiPreferences: defaultAppUiPreferencesFromPrefs(preferencesStore.get()),
};