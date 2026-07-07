import { useMemo, useSyncExternalStore } from 'react';

import {
    componentActionTitle,
    dockerDeployTitle,
    dockerInstallTitle,
    systemPackageTitle,
} from '../../core/domain/task-queue/labels';
import type { TaskQueueItem, TaskQueueSnapshot, TaskQueueStatus } from '../../core/domain/task-queue/types';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ComponentId, DeploymentTaskSnapshot } from '../../core/ipc/types';
import { deploymentTaskStore } from './deploymentTaskStore';

export interface UseTaskQueueOptions {
    /** host_id → 展示名；不传则用 hostId */
    hostLabels?: Record<string, string>;
}

function hostLabelFor(hostId: string, hostLabels?: Record<string, string>): string {
    return hostLabels?.[hostId]?.trim() || hostId;
}

function toNumberMs(value: bigint | number | null | undefined): number {
    if (value == null) return 0;
    return typeof value === 'bigint' ? Number(value) : value;
}

function progressFromTask(task: DeploymentTaskSnapshot): ActionProgressView | null {
    if (task.progressEvents.length === 0) return null;
    return task.progressEvents.reduce(
        (progress, event) => reduceActionProgress(progress, event),
        initialActionProgress,
    );
}

function statusFromTask(task: DeploymentTaskSnapshot, progress: ActionProgressView | null): TaskQueueStatus {
    switch (task.status) {
        case 'queued':
            return 'pending';
        case 'running':
            return progress?.status ?? 'running';
        case 'waiting_input':
            return 'paused';
        case 'success':
            return 'success';
        case 'failed':
            return 'failed';
        case 'cancelled':
            return 'cancelled';
        default:
            return 'pending';
    }
}

function isActiveStatus(status: TaskQueueStatus): boolean {
    return (
        status === 'pending' ||
        status === 'running' ||
        status === 'paused' ||
        status === 'installing'
    );
}

function activeRank(status: TaskQueueStatus): number {
    switch (status) {
        case 'running':
            return 0;
        case 'paused':
            return 1;
        case 'installing':
            return 2;
        case 'pending':
            return 3;
        default:
            return 10;
    }
}

function sortTaskItems(a: TaskQueueItem, b: TaskQueueItem): number {
    const aActive = isActiveStatus(a.status);
    const bActive = isActiveStatus(b.status);
    if (aActive !== bActive) return aActive ? -1 : 1;
    if (aActive && bActive) {
        const rank = activeRank(a.status) - activeRank(b.status);
        if (rank !== 0) return rank;
    }
    return b.startedAt - a.startedAt;
}

function taskToItem(
    task: DeploymentTaskSnapshot,
    hostLabels?: Record<string, string>,
): TaskQueueItem {
    const progress = progressFromTask(task);
    const status = statusFromTask(task, progress);
    const hostLabel = hostLabelFor(task.hostId, hostLabels);
    const startedAt =
        toNumberMs(task.startedAtMs) || toNumberMs(task.submittedAtMs);
    const endedAt = task.endedAtMs == null ? undefined : toNumberMs(task.endedAtMs);

    switch (task.kind.kind) {
        case 'component_action': {
            const componentId = task.kind.component_id as ComponentId;
            return {
                id: task.taskId,
                kind: 'component_action',
                title: componentActionTitle(componentId, task.kind.action, progress?.message || task.title),
                hostId: task.hostId,
                hostLabel,
                status,
                startedAt,
                endedAt,
                progress,
                logHint: task.message ?? null,
                cancellable: task.cancellable,
            };
        }
        case 'system_package':
            return {
                id: task.taskId,
                kind: 'system_package',
                title: systemPackageTitle(task.kind.package_group, task.title),
                hostId: task.hostId,
                hostLabel,
                status,
                startedAt,
                endedAt,
                progress,
                logHint: task.message ?? null,
                cancellable: task.cancellable,
            };
        case 'docker_install':
            return {
                id: task.taskId,
                kind: 'docker_install',
                title: dockerInstallTitle(hostLabel),
                hostId: task.hostId,
                hostLabel,
                status,
                startedAt,
                endedAt,
                progress,
                logHint: task.message ?? null,
                cancellable: task.cancellable,
            };
        case 'docker_image_pull':
            return {
                id: task.taskId,
                kind: 'docker_deploy',
                title: dockerDeployTitle(hostLabel, task.kind.flavor),
                hostId: task.hostId,
                hostLabel,
                status,
                startedAt,
                endedAt,
                progress,
                logHint: task.message ?? null,
                cancellable: task.cancellable,
            };
        default:
            return {
                id: task.taskId,
                kind: 'component_action',
                title: task.title,
                hostId: task.hostId,
                hostLabel,
                status,
                startedAt,
                endedAt,
                progress,
                logHint: task.message ?? null,
                cancellable: task.cancellable,
            };
    }
}

export function useTaskQueue(options?: UseTaskQueueOptions): TaskQueueSnapshot {
    const taskState = useSyncExternalStore(
        deploymentTaskStore.subscribe,
        deploymentTaskStore.getSnapshot,
        deploymentTaskStore.getSnapshot,
    );
    const hostLabels = options?.hostLabels;

    return useMemo(() => {
        const items = Object.values(taskState.tasks)
            .map((task) => taskToItem(task, hostLabels));
        items.sort(sortTaskItems);
        const activeCount = items.filter((i) => isActiveStatus(i.status)).length;
        return { items, activeCount };
    }, [taskState, hostLabels]);
}
