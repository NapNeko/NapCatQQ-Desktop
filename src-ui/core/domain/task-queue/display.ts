// 任务队列展示用纯函数（列表 / 详情 / 页头共用）。

import type { TaskQueueItem, TaskQueueStatus } from './types';

export function isActiveTaskStatus(status: TaskQueueStatus): boolean {
    return (
        status === 'pending' ||
        status === 'running' ||
        status === 'paused' ||
        status === 'installing'
    );
}

export function isActiveTaskItem(item: TaskQueueItem): boolean {
    return isActiveTaskStatus(item.status);
}

export function isTerminalTaskStatus(status: TaskQueueStatus): boolean {
    return !isActiveTaskStatus(status);
}

export function isTerminalTaskItem(item: TaskQueueItem): boolean {
    return isTerminalTaskStatus(item.status);
}

export function canCancelTaskItem(item: TaskQueueItem): boolean {
    if (item.status === 'pending' || item.status === 'paused') return true;
    if (item.status === 'running' || item.status === 'installing') {
        return item.cancellable === true;
    }
    return false;
}

export function isRunningTaskItem(item: TaskQueueItem): boolean {
    return item.status === 'running' || item.status === 'installing';
}

export function statusLabel(status: TaskQueueStatus): string {
    switch (status) {
        case 'running':
            return '进行中';
        case 'pending':
            return '等待中';
        case 'paused':
            return '已暂停';
        case 'installing':
            return '安装中';
        case 'success':
            return '已完成';
        case 'failed':
            return '失败';
        case 'cancelled':
            return '已取消';
        default:
            return status;
    }
}

export function statusShort(status: TaskQueueStatus): string {
    switch (status) {
        case 'running':
            return '进行中';
        case 'pending':
            return '等待';
        case 'paused':
            return '暂停';
        case 'installing':
            return '安装中';
        case 'success':
            return '完成';
        case 'failed':
            return '失败';
        case 'cancelled':
            return '取消';
        default:
            return status;
    }
}

export function statusTone(
    status: TaskQueueStatus,
): 'brand' | 'success' | 'danger' | 'warning' | 'neutral' {
    switch (status) {
        case 'running':
        case 'installing':
            return 'brand';
        case 'success':
            return 'success';
        case 'failed':
            return 'danger';
        case 'cancelled':
            return 'warning';
        default:
            return 'neutral';
    }
}

export function kindBadgeTone(
    kind: TaskQueueItem['kind'],
): 'brand' | 'success' | 'danger' | 'warning' | 'neutral' | 'info' {
    switch (kind) {
        case 'component_action':
            return 'brand';
        case 'system_package':
            return 'success';
        case 'docker_install':
            return 'info';
        case 'docker_deploy':
            return 'warning';
        default:
            return 'neutral';
    }
}

export function kindLabel(kind: TaskQueueItem['kind']): string {
    switch (kind) {
        case 'component_action':
            return '组件';
        case 'system_package':
            return '系统依赖';
        case 'docker_install':
            return 'Docker 安装';
        case 'docker_deploy':
            return 'Docker 部署';
        default:
            return kind;
    }
}

export function getTaskEndedAt(progress: TaskQueueItem['progress'], itemEndedAt?: number): number | undefined {
    if (itemEndedAt !== undefined) return itemEndedAt;
    if (!progress) return undefined;
    if (
        progress.status === 'success' ||
        progress.status === 'failed' ||
        progress.status === 'cancelled'
    ) {
        if (progress.logs.length > 0) {
            return progress.logs[progress.logs.length - 1].timestamp_ms;
        }
    }
    return undefined;
}

/** 详情区：中文可读时长 */
export function formatElapsedLong(startedAt: number, endedAt?: number, now = Date.now()): string {
    if (startedAt <= 0) return '—';
    const endTime = endedAt ?? now;
    const sec = Math.max(0, Math.floor((endTime - startedAt) / 1000));
    if (sec < 60) return `${sec} 秒`;
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return s > 0 ? `${m} 分 ${s} 秒` : `${m} 分`;
}

/** 列表行：紧凑 mm:ss */
export function formatElapsedCompact(startedAt: number, endedAt?: number, now = Date.now()): string {
    if (startedAt <= 0) return '';
    const endTime = endedAt ?? now;
    const sec = Math.max(0, Math.floor((endTime - startedAt) / 1000));
    if (sec < 3600) {
        const m = Math.floor(sec / 60);
        const s = sec % 60;
        return m > 0 ? `${m}:${String(s).padStart(2, '0')}` : `${s}s`;
    }
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    return `${h}h${m}m`;
}

export function failureHint(item: TaskQueueItem): string | null {
    if (item.status !== 'failed') return null;
    const msg = item.progress?.message?.trim();
    if (msg) return msg;
    const lastLog = item.progress?.logs[item.progress.logs.length - 1]?.message?.trim();
    return lastLog || '任务失败，请查看日志或到组件页重试。';
}
