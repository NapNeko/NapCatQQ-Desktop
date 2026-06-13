// 模块级 docker 安装进度（与 dockerDeployProgressStore 对称）。

import { createStore } from '../utils/createStore';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ProgressEvent } from '../../core/ipc/types';
import { scheduleTaskQueueTerminalCleanup } from '../task-queue/taskQueueTerminalLinger';

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

export const dockerInstallProgressStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

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

        const isTerminal =
            next.status === 'success' ||
            next.status === 'failed' ||
            next.status === 'cancelled';
        if (isTerminal) {
            scheduleTaskQueueTerminalCleanup(taskId, lingerTimers, (id) => {
                const s = store.getSnapshot();
                const cleanedTasks = { ...s.tasks };
                const cleanedHosts = { ...s.hostByTaskId };
                delete cleanedTasks[id];
                delete cleanedHosts[id];
                store.setState({ tasks: cleanedTasks, hostByTaskId: cleanedHosts });
            });
        }
    },

    _reset(): void {
        for (const t of lingerTimers.values()) clearTimeout(t);
        lingerTimers.clear();
        store._reset();
    },
};