// 任务队列自动清理偏好（app-settings，保存设置后更新）。

import { useSyncExternalStore } from 'react';
import {
    DEFAULT_TASK_QUEUE_CLEANUP,
    taskQueueCleanupFromAppSettings,
    type TaskQueueCleanupPrefs,
} from '../../core/domain/task-queue/cleanup';
import { trimAllProgressStoresWhenAutoCleanupOff } from './trimProgressStoresWhenAutoCleanupOff';

let prefs: TaskQueueCleanupPrefs = { ...DEFAULT_TASK_QUEUE_CLEANUP };
const listeners = new Set<() => void>();

function notify() {
    for (const fn of listeners) fn();
}

function afterPrefsApplied(): void {
    if (!prefs.taskQueueCleanupEnabled) {
        trimAllProgressStoresWhenAutoCleanupOff();
    }
}

export const taskQueueCleanupPrefsStore = {
    getSnapshot(): TaskQueueCleanupPrefs {
        return prefs;
    },
    subscribe(listener: () => void): () => void {
        listeners.add(listener);
        return () => listeners.delete(listener);
    },
    applyFromAppSettings(slice: {
        taskQueueCleanupEnabled?: boolean;
        taskQueueCleanupLingerMs?: bigint | number;
    }): void {
        prefs = taskQueueCleanupFromAppSettings(slice);
        afterPrefsApplied();
        notify();
    },
    applyPrefs(next: TaskQueueCleanupPrefs): void {
        prefs = {
            taskQueueCleanupEnabled: next.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: next.taskQueueCleanupLingerMs,
        };
        afterPrefsApplied();
        notify();
    },
    _reset(): void {
        prefs = { ...DEFAULT_TASK_QUEUE_CLEANUP };
        notify();
    },
};

export function useTaskQueueCleanupPrefs(): TaskQueueCleanupPrefs {
    return useSyncExternalStore(
        taskQueueCleanupPrefsStore.subscribe,
        taskQueueCleanupPrefsStore.getSnapshot,
        taskQueueCleanupPrefsStore.getSnapshot,
    );
}