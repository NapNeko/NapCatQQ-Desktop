// 终态任务延迟移除：读取 taskQueueCleanupPrefsStore，供各 progress store 共用。

import {
    shouldScheduleTaskQueueTerminalCleanup,
    TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF,
    taskQueueTerminalLingerMs,
    trimTerminalTasksInRecord,
    type TrimTerminalTasksResult,
} from '../../core/domain/task-queue/cleanup';
import { taskQueueCleanupPrefsStore } from './taskQueueCleanupPrefsStore';

export function getTaskQueueTerminalLingerMs(): number | null {
    const prefs = taskQueueCleanupPrefsStore.getSnapshot();
    if (!shouldScheduleTaskQueueTerminalCleanup(prefs)) return null;
    return taskQueueTerminalLingerMs(prefs);
}

export function scheduleTaskQueueTerminalCleanup(
    taskId: string,
    lingerTimers: Map<string, ReturnType<typeof setTimeout>>,
    onExpire: (taskId: string) => void,
): void {
    if (lingerTimers.has(taskId)) return;
    const ms = getTaskQueueTerminalLingerMs();
    if (ms === null) return;
    const timer = setTimeout(() => {
        lingerTimers.delete(taskId);
        onExpire(taskId);
    }, ms);
    lingerTimers.set(taskId, timer);
}

export function trimTerminalTasksWhenAutoCleanupOff<T>(
    tasks: Record<string, T>,
    isTerminal: (task: T) => boolean,
): TrimTerminalTasksResult<T> {
    if (getTaskQueueTerminalLingerMs() !== null) {
        return { tasks, removedIds: [] };
    }
    return trimTerminalTasksInRecord(
        tasks,
        isTerminal,
        TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF,
    );
}