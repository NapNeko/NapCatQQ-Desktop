// docker 安装 / 拉镜像：按主机（+ 口味）锁定，避免重复提交与任务队列重复项。

import { createStore } from '../utils/createStore';
import { taskQueueMetaStore } from '../task-queue/taskQueueMetaStore';
import type { DockerFlavor } from '../../core/ipc/types';

const INSTALL_HINT = '正在安装 Docker…';
const PULL_HINT = '正在拉取框架镜像…';

export function dockerPullTargetKey(hostId: string, flavor: DockerFlavor): string {
    return `${hostId}::${flavor}`;
}

export interface DockerActionStoreState {
    installingByHost: Record<string, boolean>;
    installHintByHost: Record<string, string>;
    /** host_id → 当前安装 task_id */
    installTaskIdByHost: Record<string, string>;
    /** hostId::flavor → 是否正在拉镜像 */
    pullingByTarget: Record<string, boolean>;
    pullHintByTarget: Record<string, string>;
    /** hostId::flavor → task_id */
    pullTaskIdByTarget: Record<string, string>;
}

const initialState: DockerActionStoreState = {
    installingByHost: {},
    installHintByHost: {},
    installTaskIdByHost: {},
    pullingByTarget: {},
    pullHintByTarget: {},
    pullTaskIdByTarget: {},
};

const store = createStore<DockerActionStoreState>(initialState);

export const dockerActionStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

    markInstalling(hostId: string, taskId: string, hint: string = INSTALL_HINT): void {
        const current = store.getSnapshot();
        store.setState({
            ...current,
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
            ...current,
            installingByHost: nextInstalling,
            installHintByHost: nextHint,
            installTaskIdByHost: nextTask,
        });
        taskQueueMetaStore.clearDockerInstall(hostId);
    },

    isPulling(hostId: string, flavor: DockerFlavor): boolean {
        return !!store.getSnapshot().pullingByTarget[dockerPullTargetKey(hostId, flavor)];
    },

    markPulling(hostId: string, flavor: DockerFlavor, taskId: string, hint: string = PULL_HINT): void {
        const key = dockerPullTargetKey(hostId, flavor);
        const current = store.getSnapshot();
        store.setState({
            ...current,
            pullingByTarget: { ...current.pullingByTarget, [key]: true },
            pullHintByTarget: { ...current.pullHintByTarget, [key]: hint },
            pullTaskIdByTarget: { ...current.pullTaskIdByTarget, [key]: taskId },
        });
    },

    clearPulling(hostId: string, flavor: DockerFlavor): void {
        const key = dockerPullTargetKey(hostId, flavor);
        const current = store.getSnapshot();
        if (!current.pullingByTarget[key]) return;
        const nextPull = { ...current.pullingByTarget };
        const nextHint = { ...current.pullHintByTarget };
        const nextTask = { ...current.pullTaskIdByTarget };
        delete nextPull[key];
        delete nextHint[key];
        delete nextTask[key];
        store.setState({
            ...current,
            pullingByTarget: nextPull,
            pullHintByTarget: nextHint,
            pullTaskIdByTarget: nextTask,
        });
    },

    clearPullingByTaskId(taskId: string): void {
        const current = store.getSnapshot();
        for (const [key, tid] of Object.entries(current.pullTaskIdByTarget)) {
            if (tid !== taskId) continue;
            const nextPull = { ...current.pullingByTarget };
            const nextHint = { ...current.pullHintByTarget };
            const nextTask = { ...current.pullTaskIdByTarget };
            delete nextPull[key];
            delete nextHint[key];
            delete nextTask[key];
            store.setState({
                ...current,
                pullingByTarget: nextPull,
                pullHintByTarget: nextHint,
                pullTaskIdByTarget: nextTask,
            });
            return;
        }
    },

    _reset(): void {
        store._reset();
    },
};