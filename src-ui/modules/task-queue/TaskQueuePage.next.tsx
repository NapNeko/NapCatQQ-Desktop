// 任务队列独立页：左侧任务轨 + 右侧详情工作台。

import React, { useMemo, useState } from 'react';
import { cn } from '../../shared/utils/cn';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import { isActiveTaskItem } from '../../core/domain/task-queue/display';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import { TaskQueueListItem } from './TaskQueueListItem';
import { TaskDetailPanel } from './TaskDetailPanel';
import { TaskQueueEmptyState } from './TaskQueueEmptyState';
import { useTaskQueueSelection } from './useTaskQueueSelection';

export type TaskQueueFilter = 'all' | 'active' | 'done';

function filterItems(items: TaskQueueItem[], filter: TaskQueueFilter): TaskQueueItem[] {
    switch (filter) {
        case 'active':
            return items.filter(isActiveTaskItem);
        case 'done':
            return items.filter((i) => !isActiveTaskItem(i));
        default:
            return items;
    }
}

type FilterDef = { id: TaskQueueFilter; label: string };

const FILTERS: FilterDef[] = [
    { id: 'all', label: '全部' },
    { id: 'active', label: '进行中' },
    { id: 'done', label: '已结束' },
];

export interface TaskQueuePageNextProps {
    items: TaskQueueItem[];
    activeCount: number;
    onNavigate?: (route: AppRoute) => void;
}

export const TaskQueuePageNext: React.FC<TaskQueuePageNextProps> = ({
    items,
    activeCount,
    onNavigate,
}) => {
    const [filter, setFilter] = useState<TaskQueueFilter>('all');
    const filtered = useMemo(() => filterItems(items, filter), [items, filter]);
    const { selectedId, setSelectedId, selected } = useTaskQueueSelection(filtered);

    const counts = useMemo(
        () => ({
            all: items.length,
            active: items.filter(isActiveTaskItem).length,
            done: items.filter((i) => !isActiveTaskItem(i)).length,
        }),
        [items],
    );

    const showWorkbench = items.length > 0;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">tasks</p>
                    <h1 className="font-display text-xl font-semibold text-text">任务队列</h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        组件与 Docker 相关操作的全局进度；切换页面后任务仍在后台执行。
                    </p>
                </div>
            </header>

            {showWorkbench && (
                <div
                    className="mb-4 flex shrink-0 flex-wrap items-center gap-2"
                    role="tablist"
                    aria-label="任务筛选"
                >
                    <div className="inline-flex min-w-0 flex-wrap items-center gap-0.5 rounded-md bg-inset p-0.5">
                        {FILTERS.map((f) => {
                            const on = filter === f.id;
                            const count = counts[f.id];
                            return (
                                <button
                                    key={f.id}
                                    type="button"
                                    role="tab"
                                    aria-selected={on}
                                    onClick={() => setFilter(f.id)}
                                    className={cn(
                                        'inline-flex items-center gap-1.5 rounded-sm px-2.5 py-1 text-[12px] font-medium transition-colors',
                                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                        on
                                            ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                            : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                                    )}
                                >
                                    <span>{f.label}</span>
                                    <span
                                        className={cn(
                                            'min-w-[1.25rem] rounded-pill px-1 py-px text-center text-[10px] font-semibold tabular-nums leading-none',
                                            on
                                                ? 'bg-brand-soft text-brand'
                                                : 'bg-muted/60 text-text-tertiary',
                                        )}
                                    >
                                        {count}
                                    </span>
                                </button>
                            );
                        })}
                    </div>
                    {activeCount > 0 && (
                        <span className="text-[11px] text-text-tertiary">
                            <span className="font-medium tabular-nums text-brand">{activeCount}</span>
                            {' '}
                            条进行中
                        </span>
                    )}
                </div>
            )}

            {!showWorkbench ? (
                <TaskQueueEmptyState variant="no-tasks" onNavigate={onNavigate} />
            ) : (
            <div
                className={cn(
                    'flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border-subtle',
                    'bg-[color-mix(in_srgb,var(--surface-canvas)_82%,var(--surface-inset)_18%)]',
                )}
            >
                {filtered.length === 0 ? (
                    <TaskQueueEmptyState variant="no-filter-match" />
                ) : (
                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden lg:grid lg:grid-cols-[minmax(260px,320px)_minmax(0,1fr)]">
                        <aside className="flex min-h-0 flex-col border-b border-border-subtle lg:border-b-0 lg:border-r">
                            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border-subtle/70 px-3 py-2.5">
                                <span className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
                                    任务列表
                                </span>
                                {activeCount > 0 && (
                                    <span className="rounded-pill bg-brand-soft px-2 py-0.5 text-[10px] font-medium text-brand">
                                        {activeCount} 进行中
                                    </span>
                                )}
                            </div>
                            <div className="scrollbar-hide min-h-0 flex-1 overflow-y-auto p-2">
                                <ul className="flex flex-col gap-0.5" role="list">
                                    {filtered.map((item) => (
                                        <li key={item.id}>
                                            <TaskQueueListItem
                                                item={item}
                                                selected={item.id === selectedId}
                                                onSelect={() => setSelectedId(item.id)}
                                            />
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        </aside>

                        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                            {selected ? (
                                <TaskDetailPanel item={selected} />
                            ) : (
                                <div className="flex flex-1 items-center justify-center p-8 text-[13px] text-text-secondary">
                                    从左侧选择一条任务查看详情
                                </div>
                            )}
                        </section>
                    </div>
                )}
            </div>
            )}
        </div>
    );
};