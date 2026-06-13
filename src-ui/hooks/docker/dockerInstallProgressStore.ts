// 模块级 docker 安装进度（与 dockerDeployProgressStore 对称）。

import { createStore } from '../utils/createStore';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ProgressEvent } from '../../core/ipc/types';
import { scheduleTaskQueueTerminalCleanup, trimTerminalTasksWhenAutoCleanupOff } from '../task-queue/taskQueueTerminalLinger';

export interface DockerInstallProgressStoreState {
    tasks: Record<string, ActionProgressView>;
    /** task_id → host_id */
    hostByTaskId: Record<string, string>;
}

const initialState: DockerInstallProgressStoreState = {
    tasks: {},
    hostByTaskId: {},
};

const store = createStore<DockerInstallProgressStoreState>(initialState);
const lingerTimers = new Map<string, ReturnType<typeof setTimeout>>();

function isTerminalStatus(status: ActionProgressView['status']): boolean {
    return status === 'success' || status === 'failed' || status === 'cancelled';
}

function applyTrimWhenAutoCleanupOff(): void {
    const snap = store.getSnapshot();
    const { tasks: trimmed, removedIds } = trimTerminalTasksWhenAutoCleanupOff(
        snap.tasks,
        (t) => isTerminalStatus(t.status),
    );
    if (removedIds.length === 0) return;
    for (const id of removedIds) {
        if (lingerTimers.has(id)) {
            clearTimeout(lingerTimers.get(id)!);
            lingerTimers.delete(id);
        }
    }
    const cleanedHosts = { ...snap.hostByTaskId };
    for (const id of removedIds) delete cleanedHosts[id];
    store.setState({ tasks: trimmed, hostByTaskId: cleanedHosts });
}

function onInstallTerminal(taskId: string): void {
    scheduleTaskQueueTerminalCleanup(taskId, lingerTimers, (id) => {
        const s = store.getSnapshot();
        const cleanedTasks = { ...s.tasks };
        const cleanedHosts = { ...s.hostByTaskId };
        delete cleanedTasks[id];
        delete cleanedHosts[id];
        store.setState({ tasks: cleanedTasks, hostByTaskId: cleanedHosts });
    });
    applyTrimWhenAutoCleanupOff();
}

export const dockerInstallProgressStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

    trimTerminalTasksWhenAutoCleanupOff: applyTrimWhenAutoCleanupOff,

    started(taskId: string, hostId: string): void {
        const current = store.getSnapshot();
        if (lingerTimers.has(taskId)) {
            clearTimeout(lingerTimers.get(taskId)!);
            lingerTimers.delete(taskId);
        }
        store.setState({
            tasks: { ...current.tasks, [taskId]: initialActionProgress },
            hostByTaskId: { ...current.hostByTaskId, [taskId]: hostId },
        });
    },

    applyProgress(taskId: string, event: ProgressEvent): void {
        const current = store.getSnapshot();
        const prev = current.tasks[taskId] ?? initialActionProgress;
        const next = reduceActionProgress(prev, event);
        store.setState({
            ...current,
            tasks: { ...current.tasks, [taskId]: next },
        });

        if (isTerminalStatus(next.status)) {
            onInstallTerminal(taskId);
        }
    },

    _reset(): void {
        for (const t of lingerTimers.values()) clearTimeout(t);
        lingerTimers.clear();
        store._reset();
    },
};