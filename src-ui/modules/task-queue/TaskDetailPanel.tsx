// 任务详情：状态摘要 + 进度 + 步骤日志。

import React from 'react';
import { cn } from '../../shared/utils/cn';
import { Badge, Button } from '../../shared/ui';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import {
    failureHint,
    formatElapsedLong,
    getTaskEndedAt,
    isActiveTaskStatus,
    kindBadgeTone,
    kindLabel,
    statusLabel,
    statusTone,
} from '../../core/domain/task-queue/display';
import { useNowMs } from '../../hooks/ui/useNowMs';
import { Loader2, XCircle } from 'lucide-react';
import { MotionIcon } from '../../shared/ui/motion';
import { ProgressLine, shouldShowProgressBar, ProgressBarOverlay } from '../components/progressView';
import { DockerPullLayersPanel } from '../components/DockerPullLayersPanel';
import { shouldShowDockerPullLayersInTaskDetail, shouldShowStepLogsInTaskDetail } from '../../core/domain/components/dockerPullProgress';
import type { ActionProgressView } from '../../core/domain/components/progress';
import { deploymentTaskService } from '../../core/services/deployment-task.service';

function DockerDeployProgressBlock({
    item,
    progress,
    expanded = false,
}: {
    item: TaskQueueItem;
    progress: ActionProgressView;
    /** 无步骤日志时占满详情下半区 */
    expanded?: boolean;
}) {
    const showLayers = shouldShowDockerPullLayersInTaskDetail(item.kind, progress);

    if (progress.status === 'failed' || progress.status === 'cancelled') {
        return (
            <p className="text-[12px] text-text-tertiary">
                {progress.status === 'cancelled' ? '已取消' : '失败 · 见上方失败原因'}
            </p>
        );
    }
    if (progress.status === 'success') {
        return <p className="text-[12px] text-success">镜像已就绪</p>;
    }

    const rootClass = expanded
        ? 'flex min-h-0 flex-1 flex-col'
        : undefined;

    return (
        <div className={rootClass}>
            <div className="flex shrink-0 min-w-0 items-center gap-2">
                <MotionIcon
                    icon={Loader2}
                    motion="spin"
                    playEnter={false}
                    size={12}
                    className="shrink-0 text-brand"
                />
                <span className="min-w-0 truncate text-[12px] text-text-secondary">
                    {progress.message || '拉取镜像…'}
                </span>
                <span className="ml-auto shrink-0 font-mono text-[11.5px] tabular-nums text-text-secondary">
                    {progress.percent}%
                </span>
            </div>
            {shouldShowProgressBar(progress) && (
                <div className="relative mt-2 h-1.5 w-full shrink-0 overflow-hidden rounded-pill bg-inset/60">
                    <ProgressBarOverlay progress={progress} determinate={progress.dockerLayers.length > 0} />
                </div>
            )}
            {showLayers && (
                <DockerPullLayersPanel
                    progress={progress}
                    fillHeight={expanded}
                    showWaitingPlaceholder={
                        progress.dockerLayers.length === 0 &&
                        progress.status === 'running' &&
                        progress.currentStep === 2
                    }
                />
            )}
        </div>
    );
}

export interface TaskDetailPanelProps {
    item: TaskQueueItem;
}

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
        <div className="scrollbar-hide min-h-0 flex-1 overflow-y-auto px-3 py-3 font-mono text-[12px] leading-[1.55] antialiased">
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

export const TaskDetailPanel: React.FC<TaskDetailPanelProps> = ({ item }) => {
    const progress = item.progress;
    const failure = failureHint(item);
    const endedAt = getTaskEndedAt(progress, item.endedAt);
    const ticking = isActiveTaskStatus(item.status) && endedAt === undefined;
    const nowMs = useNowMs(ticking);
    const showStepLogs = shouldShowStepLogsInTaskDetail(item.kind);
    const dockerPullExpanded = item.kind === 'docker_deploy' && !showStepLogs;
    const canCancel = item.cancellable === true && isActiveTaskStatus(item.status);

    const handleCancel = () => {
        void deploymentTaskService.cancel(item.id).catch((err) => {
            console.error('[TaskQueue] cancel failed:', err);
        });
    };

    return (
        <div className="flex h-full min-h-0 flex-1 flex-col">
            <div className="shrink-0 border-b border-border-subtle/70 px-4 py-4 sm:px-5">
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="font-display text-lg font-semibold leading-tight text-text">
                                {item.title}
                            </h2>
                            <Badge tone={kindBadgeTone(item.kind)} appearance="soft" className="text-[11px]">
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
                            <span className="tabular-nums">
                                {formatElapsedLong(
                                    item.startedAt,
                                    endedAt,
                                    endedAt === undefined ? nowMs : undefined,
                                )}
                            </span>
                        </p>
                    </div>
                    {canCancel && (
                        <Button size="sm" variant="secondary" onClick={handleCancel}>
                            <XCircle size={13} />
                            取消
                        </Button>
                    )}
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

                {progress && !dockerPullExpanded && (
                    <div className="mt-3 overflow-hidden rounded-md border border-border-subtle/80 bg-surface/60 px-3 pb-3 pt-2">
                        {item.kind === 'docker_deploy' ? (
                            <DockerDeployProgressBlock item={item} progress={progress} />
                        ) : (
                            <>
                                <ProgressLine progress={progress} />
                                {shouldShowProgressBar(progress) && (
                                    <div className="relative mt-2 h-1.5 w-full overflow-hidden rounded-pill bg-inset/60">
                                        <ProgressBarOverlay progress={progress} determinate />
                                    </div>
                                )}
                            </>
                        )}
                    </div>
                )}
            </div>

            {dockerPullExpanded && progress && (
                <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 pb-4 pt-3 sm:px-5">
                    <div
                        className={cn(
                            'flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50 px-3 pb-3 pt-2.5',
                            LOG_SURFACE,
                        )}
                    >
                        <DockerDeployProgressBlock item={item} progress={progress} expanded />
                    </div>
                </div>
            )}

            {showStepLogs && (
                <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 pb-4 pt-3 sm:px-5">
                    <div className="flex shrink-0 pb-2">
                        <span className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
                            步骤日志
                        </span>
                    </div>

                    <div
                        className={cn(
                            'flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50',
                            LOG_SURFACE,
                        )}
                    >
                        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                            <StepLogBody item={item} />
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
};
