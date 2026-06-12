// 任务队列列表行。

import React from 'react';
import { Box, Container, Download } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';

function KindIcon({ kind }: { kind: TaskQueueItem['kind'] }) {
    switch (kind) {
        case 'component_action':
            return <Box size={14} className="shrink-0 text-text-tertiary" />;
        case 'docker_install':
            return <Download size={14} className="shrink-0 text-text-tertiary" />;
        case 'docker_deploy':
            return <Container size={14} className="shrink-0 text-text-tertiary" />;
    }
}

function statusTone(item: TaskQueueItem): 'brand' | 'success' | 'danger' | 'warning' | 'neutral' {
    switch (item.status) {
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

function statusChip(item: TaskQueueItem): string {
    switch (item.status) {
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
            return item.status;
    }
}

function formatElapsed(startedAt: number, endedAt?: number): string {
    if (startedAt <= 0) return '';
    const endTime = endedAt || Date.now();
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

function getEndedAt(progress: TaskQueueItem['progress']): number | undefined {
    if (!progress) return undefined;
    if (progress.status === 'success' || progress.status === 'failed' || progress.status === 'cancelled') {
        if (progress.logs.length > 0) {
            return progress.logs[progress.logs.length - 1].timestamp_ms;
        }
    }
    return undefined;
}

export interface TaskQueueListItemProps {
    item: TaskQueueItem;
    selected: boolean;
    onSelect: () => void;
}

export const TaskQueueListItem: React.FC<TaskQueueListItemProps> = ({ item, selected, onSelect }) => {
    return (
        <button
            type="button"
            onClick={onSelect}
            className={cn(
                'flex w-full items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors',
                selected
                    ? 'border-info/40 bg-info-soft/50'
                    : 'border-border-subtle bg-surface/40 hover:bg-surface/80',
            )}
        >
            <KindIcon kind={item.kind} />
            <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium text-text">{item.title}</div>
                <div className="mt-0.5 truncate text-[11px] text-text-tertiary">
                    {item.hostLabel}
                    {item.startedAt > 0 ? ` · ${formatElapsed(item.startedAt, getEndedAt(item.progress))}` : ''}
                </div>
            </div>
            <Badge tone={statusTone(item)} appearance="soft" className="shrink-0">
                {statusChip(item)}
            </Badge>
        </button>
    );
};