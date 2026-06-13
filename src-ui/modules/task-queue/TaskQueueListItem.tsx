// 任务队列列表行（左侧任务轨）。

import React from 'react';
import { Box, Container, Download } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import { ActionMotionIcon, LIVE_MOTION, RESOURCE_MOTION, type MotionIconPreset } from '../../shared/ui/motion';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import {
    formatElapsedCompact,
    getTaskEndedAt,
    isActiveTaskStatus,
    kindLabel,
    statusShort,
    statusTone,
} from '../../core/domain/task-queue/display';

type KindMeta = {
    Icon: typeof Box;
    motion: MotionIconPreset;
    tileSelected: string;
    iconSelected: string;
};

const KIND_META: Record<TaskQueueItem['kind'], KindMeta> = {
    component_action: {
        Icon: Box,
        motion: RESOURCE_MOTION,
        tileSelected: 'bg-brand-soft/70',
        iconSelected: 'text-brand',
    },
    docker_install: {
        Icon: Download,
        motion: LIVE_MOTION,
        tileSelected: 'bg-info-soft/80',
        iconSelected: 'text-info',
    },
    docker_deploy: {
        Icon: Container,
        motion: RESOURCE_MOTION,
        tileSelected: 'bg-accent-soft/80',
        iconSelected: 'text-accent',
    },
};

function KindIcon({
    kind,
    selected,
    busy,
}: {
    kind: TaskQueueItem['kind'];
    selected: boolean;
    busy: boolean;
}) {
    const { Icon, motion, iconSelected } = KIND_META[kind];
    return (
        <ActionMotionIcon
            icon={Icon}
            size={15}
            strokeWidth={1.75}
            motion={busy ? motion : 'none'}
            className={cn('shrink-0', selected ? iconSelected : 'text-text-tertiary')}
        />
    );
}

export interface TaskQueueListItemProps {
    item: TaskQueueItem;
    selected: boolean;
    onSelect: () => void;
}

export const TaskQueueListItem: React.FC<TaskQueueListItemProps> = ({
    item,
    selected,
    onSelect,
}) => {
    const endedAt = getTaskEndedAt(item.progress);
    const elapsed =
        item.startedAt > 0 ? formatElapsedCompact(item.startedAt, endedAt) : '';
    const busy = isActiveTaskStatus(item.status);
    const kindMeta = KIND_META[item.kind];

    return (
        <button
            type="button"
            onClick={onSelect}
            aria-current={selected ? 'true' : undefined}
            className={cn(
                'group relative flex w-full items-start gap-2.5 rounded-md px-2.5 py-2.5 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                selected
                    ? 'bg-elevated/50'
                    : 'hover:bg-elevated/25',
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'absolute bottom-2 left-0 top-2 w-[2px] rounded-r-pill transition-opacity',
                    selected ? 'bg-brand opacity-100' : 'bg-border-default opacity-0 group-hover:opacity-50',
                )}
            />
            <span
                className={cn(
                    'mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md',
                    selected ? kindMeta.tileSelected : 'bg-inset/50',
                )}
            >
                <KindIcon kind={item.kind} selected={selected} busy={busy} />
            </span>
            <div className="min-w-0 flex-1 pt-0.5">
                <div className="truncate text-[13px] font-medium leading-tight text-text">
                    {item.title}
                </div>
                <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[11px] text-text-tertiary">
                    <span className="truncate">{item.hostLabel}</span>
                    <span aria-hidden>·</span>
                    <span className="shrink-0">{kindLabel(item.kind)}</span>
                    {elapsed ? (
                        <>
                            <span aria-hidden>·</span>
                            <span className="shrink-0 tabular-nums">{elapsed}</span>
                        </>
                    ) : null}
                </div>
            </div>
            <Badge
                tone={statusTone(item.status)}
                appearance="soft"
                className="mt-0.5 shrink-0 text-[10px]"
            >
                {statusShort(item.status)}
            </Badge>
        </button>
    );
};