// docker 安装：布尔 installing + task 进度（与 dockerDeploy 对齐）。

import { createStore } from '../utils/createStore';
import { taskQueueMetaStore } from '../task-queue/taskQueueMetaStore';

const INSTALL_HINT = '正在安装 Docker…';

export interface DockerActionStoreState {
    installingByHost: Record<string, boolean>;
    installHintByHost: Record<string, string>;
    /** host_id → 当前安装 task_id */
    installTaskIdByHost: Record<string, string>;
}

const initialState: DockerActionStoreState = {
    installingByHost: {},
    installHintByHost: {},
    installTaskIdByHost: {},
};

const store = createStore<DockerActionStoreState>(initialState);

export const dockerActionStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

    markInstalling(hostId: string, taskId: string, hint: string = INSTALL_HINT): void {
        const current = store.getSnapshot();
        store.setState({
            installingByHost: { ...current.installingByHost, [hostId]: true },
            installHintByHost: { ...current.installHintByHost, [hostId]: hint },
            installTaskIdByHost: { ...current.installTaskIdByHost, [hostId]: taskId },
        });
        taskQueueMetaStore.registerDockerInstall(hostId, { hostId, startedAt: Date.now() });
    },

    clearInstalling(hostId: string): void {
        const current = store.getSnapshot();
        if (!current.installingByHost[hostId]) return;
        const nextInstalling = { ...current.installingByHost };
        const nextHint = { ...current.installHintByHost };
        const nextTask = { ...current.installTaskIdByHost };
        delete nextInstalling[hostId];
        delete nextHint[hostId];
        delete nextTask[hostId];
        store.setState({
            installingByHost: nextInstalling,
            installHintByHost: nextHint,
            installTaskIdByHost: nextTask,
        });
        taskQueueMetaStore.clearDockerInstall(hostId);
    },

    _reset(): void {
        store._reset();
    },
};