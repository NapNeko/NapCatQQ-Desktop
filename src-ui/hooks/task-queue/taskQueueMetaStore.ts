// 任务队列补全元数据（Docker 部署 taskId、安装开始时间等）。
// 阶段 2 写入；阶段 1 提供空 store 供 useTaskQueue 订阅。

import { createStore } from '../utils/createStore';
import type {
    DockerDeployMeta,
    DockerInstallMeta,
    TaskQueueMetaSnapshot,
} from '../../core/domain/task-queue/buildSnapshot';

export interface TaskQueueMetaStoreState {
    dockerDeployByTaskId: Record<string, DockerDeployMeta>;
    dockerInstallByHostId: Record<string, DockerInstallMeta>;
}

const initialState: TaskQueueMetaStoreState = {
    dockerDeployByTaskId: {},
    dockerInstallByHostId: {},
};

const store = createStore<TaskQueueMetaStoreState>(initialState);

export function toMetaSnapshot(state: TaskQueueMetaStoreState): TaskQueueMetaSnapshot {
    return {
        dockerDeployByTaskId: state.dockerDeployByTaskId,
        dockerInstallByHostId: state.dockerInstallByHostId,
    };
}

export const taskQueueMetaStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,

    registerDockerDeploy(taskId: string, meta: DockerDeployMeta): void {
        const current = store.getSnapshot();
        store.setState({
            ...current,
            dockerDeployByTaskId: {
                ...current.dockerDeployByTaskId,
                [taskId]: meta,
            },
        });
    },

    registerDockerInstall(hostId: string, meta: DockerInstallMeta = { hostId }): void {
        const current = store.getSnapshot();
        store.setState({
            ...current,
            dockerInstallByHostId: {
                ...current.dockerInstallByHostId,
                [hostId]: {
                    hostId,
                    startedAt: meta.startedAt ?? Date.now(),
                },
            },
        });
    },

    clearDockerInstall(hostId: string): void {
        const current = store.getSnapshot();
        if (!current.dockerInstallByHostId[hostId]) return;
        const next = { ...current.dockerInstallByHostId };
        delete next[hostId];
        store.setState({ ...current, dockerInstallByHostId: next });
    },

    _reset(): void {
        store._reset();
    },
};