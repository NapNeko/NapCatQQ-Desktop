// 只读聚合三份模块级 store 快照 + meta，产出任务队列视图。

import type { ActionProgressView } from '../components/progress';
import type { ComponentId } from '../../ipc/types';
import {
    componentActionTitle,
    componentDisplayName,
    dockerDeployTitle,
    dockerInstallTitle,
} from './labels';
import type { TaskQueueItem, TaskQueueSnapshot, TaskQueueStatus } from './types';

export interface ComponentActionStoreSnapshot {
    tasks: Record<string, ActionProgressView>;
    activeByTarget: Record<string, string>;
    taskTargets: Record<string, { componentId: ComponentId; hostId: string }>;
}

export interface DockerActionStoreSnapshot {
    installingByHost: Record<string, boolean>;
    installHintByHost: Record<string, string>;
    installTaskIdByHost: Record<string, string>;
}

export interface DockerInstallProgressStoreSnapshot {
    tasks: Record<string, ActionProgressView>;
}

export interface DockerDeployProgressStoreSnapshot {
    tasks: Record<string, ActionProgressView>;
}

export interface DockerDeployMeta {
    hostId: string;
    hostLabel?: string;
    container?: string;
    flavor?: string;
}

export interface DockerInstallMeta {
    hostId: string;
    startedAt?: number;
}

export interface TaskQueueMetaSnapshot {
    dockerDeployByTaskId: Record<string, DockerDeployMeta>;
    dockerInstallByHostId: Record<string, DockerInstallMeta>;
}

export interface BuildTaskQueueSnapshotInput {
    componentAction: ComponentActionStoreSnapshot;
    dockerAction: DockerActionStoreSnapshot;
    dockerDeployProgress: DockerDeployProgressStoreSnapshot;
    dockerInstallProgress: DockerInstallProgressStoreSnapshot;
    meta: TaskQueueMetaSnapshot;
    hostLabels?: Record<string, string>;
}

function hostLabelFor(hostId: string, hostLabels?: Record<string, string>): string {
    return hostLabels?.[hostId]?.trim() || hostId;
}

function progressStatusToQueue(status: ActionProgressView['status']): TaskQueueStatus {
    return status;
}

function isActiveStatus(status: TaskQueueStatus): boolean {
    return (
        status === 'pending' ||
        status === 'running' ||
        status === 'paused' ||
        status === 'installing'
    );
}

function startedAtFromProgress(progress: ActionProgressView): number {
    if (progress.logs.length > 0) {
        return progress.logs[0].timestamp_ms;
    }
    return 0;
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

function collectComponentActionItems(
    snap: ComponentActionStoreSnapshot,
    hostLabels?: Record<string, string>,
): TaskQueueItem[] {
    const items: TaskQueueItem[] = [];
    for (const [taskId, progress] of Object.entries(snap.tasks)) {
        const target = snap.taskTargets[taskId];
        const hostId = target?.hostId ?? '';
        const componentId = target?.componentId;
        const hostLabel = hostId ? hostLabelFor(hostId, hostLabels) : '—';
        const title = componentId
            ? componentActionTitle(componentId, undefined, progress.message)
            : progress.message || taskId;
        items.push({
            id: taskId,
            kind: 'component_action',
            title,
            hostId,
            hostLabel,
            status: progressStatusToQueue(progress.status),
            startedAt: startedAtFromProgress(progress),
            progress,
            logHint: null,
        });
    }
    return items;
}

function collectDockerInstallItems(
    snap: DockerActionStoreSnapshot,
    installProgress: DockerInstallProgressStoreSnapshot,
    meta: TaskQueueMetaSnapshot,
    hostLabels?: Record<string, string>,
): TaskQueueItem[] {
    const items: TaskQueueItem[] = [];
    for (const [hostId, installing] of Object.entries(snap.installingByHost)) {
        if (!installing) continue;
        const hostLabel = hostLabelFor(hostId, hostLabels);
        const hint = snap.installHintByHost[hostId] ?? null;
        const startedAt = meta.dockerInstallByHostId[hostId]?.startedAt ?? 0;
        const taskId = snap.installTaskIdByHost[hostId];
        const progress = taskId ? installProgress.tasks[taskId] ?? null : null;
        const status: TaskQueueStatus = progress
            ? progressStatusToQueue(progress.status)
            : 'installing';
        items.push({
            id: taskId ? `docker_install::${hostId}::${taskId}` : `docker_install::${hostId}`,
            kind: 'docker_install',
            title: dockerInstallTitle(hostLabel),
            hostId,
            hostLabel,
            status,
            startedAt: progress ? startedAtFromProgress(progress) || startedAt : startedAt,
            progress,
            logHint: progress?.message?.trim() ? null : hint,
        });
    }
    for (const [taskId, progress] of Object.entries(installProgress.tasks)) {
        const hostId = Object.entries(snap.installTaskIdByHost).find(([, tid]) => tid === taskId)?.[0];
        if (!hostId) continue;
        if (snap.installingByHost[hostId]) continue;
        const hostLabel = hostLabelFor(hostId, hostLabels);
        items.push({
            id: `docker_install::${hostId}::${taskId}`,
            kind: 'docker_install',
            title: dockerInstallTitle(hostLabel),
            hostId,
            hostLabel,
            status: progressStatusToQueue(progress.status),
            startedAt: startedAtFromProgress(progress),
            progress,
            logHint: null,
        });
    }
    return items;
}

function collectDockerDeployItems(
    snap: DockerDeployProgressStoreSnapshot,
    meta: TaskQueueMetaSnapshot,
    hostLabels?: Record<string, string>,
): TaskQueueItem[] {
    const items: TaskQueueItem[] = [];
    for (const [taskId, progress] of Object.entries(snap.tasks)) {
        const deployMeta = meta.dockerDeployByTaskId[taskId];
        const hostId = deployMeta?.hostId ?? '';
        const hostLabel = hostId ? hostLabelFor(hostId, hostLabels) : '—';
        const title = dockerDeployTitle(hostLabel, deployMeta?.container);
        items.push({
            id: taskId,
            kind: 'docker_deploy',
            title,
            hostId,
            hostLabel,
            status: progressStatusToQueue(progress.status),
            startedAt: startedAtFromProgress(progress),
            progress,
            logHint: null,
        });
    }
    return items;
}

export function buildTaskQueueSnapshot(input: BuildTaskQueueSnapshotInput): TaskQueueSnapshot {
    const items = [
        ...collectComponentActionItems(input.componentAction, input.hostLabels),
        ...collectDockerInstallItems(
            input.dockerAction,
            input.dockerInstallProgress,
            input.meta,
            input.hostLabels,
        ),
        ...collectDockerDeployItems(input.dockerDeployProgress, input.meta, input.hostLabels),
    ];
    items.sort(sortTaskItems);
    const activeCount = items.filter((i) => isActiveStatus(i.status)).length;
    return { items, activeCount };
}

/** 测试 / 标题兜底：仅 componentId 时的短标题 */
export function taskTitleForComponent(componentId: ComponentId): string {
    return componentDisplayName(componentId);
}