// 任务详情：状态摘要 + 进度 + 日志工作台（步骤 / Desktop）。

import React, { useMemo, useState } from 'react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import {
    failureHint,
    formatElapsedLong,
    getTaskEndedAt,
    kindLabel,
    statusLabel,
    statusTone,
} from '../../core/domain/task-queue/display';
import { ProgressLine, shouldShowProgressBar, ProgressBarOverlay } from '../components/progressView';
import { TaskLogPanel } from './TaskLogPanel';

export interface TaskDetailPanelProps {
    item: TaskQueueItem;
    logPanelEnabled?: boolean;
    logPanelFastPoll?: boolean;
}

type LogTab = 'steps' | 'desktop';

const LOG_SURFACE =
    'bg-[color-mix(in_srgb,var(--surface-canvas)_76%,var(--surface-inset)_24%)]';

function StepLogBody({ item }: { item: TaskQueueItem }) {
    const progress = item.progress;
    if (!progress) {
        return (
            <p className="py-8 text-center text-[12px] text-text-tertiary">等待任务启动…</p>
        );
    }
    if (progress.logs.length === 0) {
        return (
            <p className="py-8 text-center text-[12px] text-text-tertiary">
                {progress.status === 'running' ? '任务运行中，暂无步骤输出' : '暂无步骤日志'}
            </p>
        );
    }
    return (
        <div className="scrollbar-hide min-h-0 flex-1 overflow-y-auto px-3 py-3 font-sans text-[12px] leading-[1.55] antialiased">
            {progress.logs.map((log, idx) => {
                const time = new Date(log.timestamp_ms).toLocaleTimeString('zh-CN', {
                    hour: '2-digit',
                    minute: '2-digit',
                    second: '2-digit',
                    hour12: false,
                });
                return (
                    <div
                        key={`${log.timestamp_ms}-${idx}`}
                        className="grid min-w-0 grid-cols-[3.5rem_minmax(0,1fr)] gap-x-2 gap-y-0.5 py-0.5 text-text-secondary"
                    >
                        <span className="tabular-nums text-[11px] text-text-tertiary">{time}</span>
                        <span className="min-w-0 break-words text-text">{log.message}</span>
                    </div>
                );
            })}
        </div>
    );
}

export const TaskDetailPanel: React.FC<TaskDetailPanelProps> = ({
    item,
    logPanelEnabled = false,
    logPanelFastPoll = false,
}) => {
    const [logTab, setLogTab] = useState<LogTab>('steps');
    const progress = item.progress;
    const failure = failureHint(item);
    const endedAt = getTaskEndedAt(progress);

    const logStartedAt = useMemo(() => {
        if (item.startedAt > 0) return item.startedAt;
        if (progress?.logs[0]?.timestamp_ms) return progress.logs[0].timestamp_ms;
        return Date.now() - 60_000;
    }, [item.startedAt, progress?.logs]);

    return (
        <div className="flex h-full min-h-0 flex-1 flex-col">
            <div className="shrink-0 border-b border-border-subtle/70 px-4 py-4 sm:px-5">
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="font-display text-lg font-semibold leading-tight text-text">
                                {item.title}
                            </h2>
                            <Badge tone="neutral" appearance="soft" className="text-[11px]">
                                {kindLabel(item.kind)}
                            </Badge>
                            <Badge tone={statusTone(item.status)} appearance="soft" className="text-[11px]">
                                {statusLabel(item.status)}
                            </Badge>
                        </div>
                        <p className="mt-2 text-[12px] text-text-secondary">
                            <span className="text-text-tertiary">主机</span>{' '}
                            <span className="font-medium text-text">{item.hostLabel}</span>
                            <span className="mx-2 text-text-disabled">|</span>
                            <span className="text-text-tertiary">耗时</span>{' '}
                            <span className="tabular-nums">{formatElapsedLong(item.startedAt, endedAt)}</span>
                        </p>
                    </div>
                </div>

                {failure && (
                    <div className="mt-3 rounded-md border border-danger/30 bg-danger-soft/35 px-3 py-2.5 text-[12px] leading-relaxed text-danger">
                        {failure}
                    </div>
                )}

                {item.kind === 'docker_install' && !progress && (
                    <div className="mt-3 rounded-md border border-border-subtle bg-inset/50 px-3 py-2.5 text-[12px] text-text-secondary">
                        {item.logHint ?? '正在安装 Docker…'}
                    </div>
                )}

                {progress && (
                    <div className="mt-3 overflow-hidden rounded-md border border-border-subtle/80 bg-surface/60 px-3 pb-3 pt-2">
                        <ProgressLine progress={progress} />
                        {shouldShowProgressBar(progress) && (
                            <div className="relative mt-2 h-1.5 w-full overflow-hidden rounded-pill bg-inset/60">
                                <ProgressBarOverlay progress={progress} />
                            </div>
                        )}
                    </div>
                )}
            </div>

            <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 pb-4 pt-3 sm:px-5">
                <div className="flex shrink-0 border-b border-border-subtle/60 pb-2">
                    <div
                        className="inline-flex gap-0.5 rounded-md bg-inset p-0.5"
                        role="tablist"
                        aria-label="日志视图"
                    >
                        {(
                            [
                                { id: 'steps' as const, label: '步骤日志' },
                                { id: 'desktop' as const, label: 'Desktop 日志' },
                            ] as const
                        ).map((t) => (
                            <button
                                key={t.id}
                                type="button"
                                role="tab"
                                aria-selected={logTab === t.id}
                                onClick={() => setLogTab(t.id)}
                                className={cn(
                                    'rounded-sm px-2.5 py-1 text-[12px] font-medium transition-colors',
                                    logTab === t.id
                                        ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                        : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                                )}
                            >
                                {t.label}
                            </button>
                        ))}
                    </div>
                </div>

                <div
                    className={cn(
                        'mt-2 flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50',
                        LOG_SURFACE,
                    )}
                    role="tabpanel"
                >
                    {logTab === 'steps' ? (
                        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                            <StepLogBody item={item} />
                        </div>
                    ) : (
                        <div className="flex min-h-0 flex-1 flex-col p-1">
                            <TaskLogPanel
                                startedAt={logStartedAt}
                                enabled={logPanelEnabled}
                                fastPoll={logPanelFastPoll}
                                fillHeight
                            />
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
};