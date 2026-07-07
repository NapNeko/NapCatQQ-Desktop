// 任务队列列表行（左侧任务轨）。

import React from 'react';
import { Trash2 } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import { ActionMotionIcon, LIVE_MOTION, RESOURCE_MOTION } from '../../shared/ui/motion';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import {
    formatElapsedCompact,
    getTaskEndedAt,
    isActiveTaskStatus,
    isTerminalTaskStatus,
    kindLabel,
    statusShort,
    statusTone,
} from '../../core/domain/task-queue/display';
import { useNowMs } from '../../hooks/ui/useNowMs';
import { TASK_KIND_VISUAL, taskKindIconClasses } from './taskQueueKindVisual';

const KIND_MOTION: Record<TaskQueueItem['kind'], typeof RESOURCE_MOTION> = {
    component_action: RESOURCE_MOTION,
    system_package: RESOURCE_MOTION,
    docker_install: LIVE_MOTION,
    docker_deploy: RESOURCE_MOTION,
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
    const { Icon } = TASK_KIND_VISUAL[kind];
    const { glyph } = taskKindIconClasses(kind, selected);
    return (
        <ActionMotionIcon
            icon={Icon}
            size={16}
            strokeWidth={selected ? 2.1 : 1.85}
            motion={busy && !selected ? KIND_MOTION[kind] : 'none'}
            className={cn('shrink-0', glyph)}
        />
    );
}

export interface TaskQueueListItemProps {
    item: TaskQueueItem;
    selected: boolean;
    onSelect: () => void;
    onDelete?: () => void;
}

export const TaskQueueListItem: React.FC<TaskQueueListItemProps> = ({
    item,
    selected,
    onSelect,
    onDelete,
}) => {
    const endedAt = getTaskEndedAt(item.progress, item.endedAt);
    const busy = isActiveTaskStatus(item.status);
    const nowMs = useNowMs(busy && endedAt === undefined);
    const elapsed =
        item.startedAt > 0
            ? formatElapsedCompact(item.startedAt, endedAt, endedAt === undefined ? nowMs : undefined)
            : '';
    const { tile } = taskKindIconClasses(item.kind, selected);
    const canDelete = isTerminalTaskStatus(item.status) && onDelete;

    return (
        <div
            aria-current={selected ? 'true' : undefined}
            className={cn(
                'group relative flex w-full items-center gap-1.5 rounded-md transition-colors',
                selected ? 'bg-elevated/50' : 'hover:bg-elevated/25',
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'absolute bottom-2 left-0 top-2 w-[2px] rounded-r-pill transition-opacity',
                    selected
                        ? 'bg-brand opacity-100'
                        : 'bg-border-default opacity-0 group-hover:opacity-50',
                )}
            />
            <button
                type="button"
                onClick={onSelect}
                className={cn(
                    'flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-2.5 py-2.5 text-left',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                )}
            >
                <span
                    className={cn(
                        'flex h-8 w-8 shrink-0 items-center justify-center rounded-md transition-colors',
                        tile,
                    )}
                >
                    <KindIcon kind={item.kind} selected={selected} busy={busy} />
                </span>
                <div className="min-w-0 flex-1">
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
            </button>
            <div className="relative mr-1 flex h-7 w-12 shrink-0 items-center justify-center">
                <Badge
                    tone={statusTone(item.status)}
                    appearance="soft"
                    className={cn(
                        'absolute max-w-full shrink-0 justify-center text-[10px] transition-opacity',
                        canDelete && 'group-hover:opacity-0 group-focus-within:opacity-0',
                    )}
                >
                    {statusShort(item.status)}
                </Badge>
                {canDelete && (
                    <button
                        type="button"
                        aria-label={`删除任务 ${item.title}`}
                        title="删除任务"
                        onClick={onDelete}
                        className={cn(
                            'absolute flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary opacity-0 transition-colors',
                            'pointer-events-none hover:bg-danger-soft hover:text-danger',
                            'focus-visible:pointer-events-auto focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                            'group-hover:pointer-events-auto group-hover:opacity-80 group-focus-within:pointer-events-auto group-focus-within:opacity-80',
                            selected && 'group-hover:opacity-100',
                        )}
                    >
                        <Trash2 size={13} strokeWidth={2} />
                    </button>
                )}
            </div>
        </div>
    );
};
