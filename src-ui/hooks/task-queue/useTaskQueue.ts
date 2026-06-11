// 全局任务队列：订阅 component / docker action / deploy progress + meta，聚合为快照。

import { useMemo, useSyncExternalStore } from 'react';

import { buildTaskQueueSnapshot } from '../../core/domain/task-queue/buildSnapshot';
import type { TaskQueueSnapshot } from '../../core/domain/task-queue/types';
import { componentActionStore } from '../components/componentActionStore';
import { dockerActionStore } from '../docker/dockerActionStore';
import { dockerDeployProgressStore } from '../docker/dockerDeployProgressStore';
import { dockerInstallProgressStore } from '../docker/dockerInstallProgressStore';
import { taskQueueMetaStore, toMetaSnapshot } from './taskQueueMetaStore';

export interface UseTaskQueueOptions {
    /** host_id → 展示名；不传则用 hostId */
    hostLabels?: Record<string, string>;
}

export function useTaskQueue(options?: UseTaskQueueOptions): TaskQueueSnapshot {
    const componentSnap = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );
    const dockerActionSnap = useSyncExternalStore(
        dockerActionStore.subscribe,
        dockerActionStore.getSnapshot,
        dockerActionStore.getSnapshot,
    );
    const dockerDeploySnap = useSyncExternalStore(
        dockerDeployProgressStore.subscribe,
        dockerDeployProgressStore.getSnapshot,
        dockerDeployProgressStore.getSnapshot,
    );
    const dockerInstallSnap = useSyncExternalStore(
        dockerInstallProgressStore.subscribe,
        dockerInstallProgressStore.getSnapshot,
        dockerInstallProgressStore.getSnapshot,
    );
    const metaSnap = useSyncExternalStore(
        taskQueueMetaStore.subscribe,
        taskQueueMetaStore.getSnapshot,
        taskQueueMetaStore.getSnapshot,
    );

    const hostLabels = options?.hostLabels;

    return useMemo(
        () =>
            buildTaskQueueSnapshot({
                componentAction: componentSnap,
                dockerAction: dockerActionSnap,
                dockerDeployProgress: dockerDeploySnap,
                dockerInstallProgress: dockerInstallSnap,
                meta: toMetaSnapshot(metaSnap),
                hostLabels,
            }),
        [componentSnap, dockerActionSnap, dockerDeploySnap, dockerInstallSnap, metaSnap, hostLabels],
    );
}