// 任务详情：进度 / 安装说明 + 日志片段。

import React from 'react';
import { Button } from '../../shared/ui';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';
import { ProgressLine, shouldShowProgressBar, ProgressBarOverlay } from '../components/progressView';
import { TaskLogPanel } from './TaskLogPanel';

export interface TaskDetailPanelProps {
    item: TaskQueueItem;
    logEnabled: boolean;
    onOpenSettingsLog: () => void;
}

function statusLabel(item: TaskQueueItem): string {
    switch (item.status) {
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
            return item.status;
    }
}

function formatElapsed(startedAt: number): string {
    if (startedAt <= 0) return '—';
    const sec = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
    if (sec < 60) return `${sec} 秒`;
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return s > 0 ? `${m} 分 ${s} 秒` : `${m} 分`;
}

export const TaskDetailPanel: React.FC<TaskDetailPanelProps> = ({
    item,
    logEnabled,
    onOpenSettingsLog,
}) => {
    const progress = item.progress;

    return (
        <div className="flex h-full min-h-0 flex-1 flex-col gap-3">
            <div className="shrink-0">
                <h3 className="font-display text-base font-semibold text-text">{item.title}</h3>
                <p className="mt-1 text-[12px] text-text-secondary">
                    {item.hostLabel} · {statusLabel(item)} · 已用 {formatElapsed(item.startedAt)}
                </p>
            </div>

            {item.kind === 'docker_install' && !progress && (
                <div className="shrink-0 rounded-md border border-border-subtle bg-surface/60 px-3 py-2 text-[12px] text-text-secondary">
                    {item.logHint ?? '正在安装 Docker…'}
                </div>
            )}

            {progress && (
                <div className="shrink-0 overflow-hidden rounded-md border border-border-subtle px-3 pt-2 pb-3">
                    <ProgressLine progress={progress} />
                    {shouldShowProgressBar(progress) && (
                        <div className="relative mt-2 h-1.5 w-full overflow-hidden rounded-pill bg-inset/60">
                            <ProgressBarOverlay progress={progress} />
                        </div>
                    )}
                    {progress.logs.length > 0 && (
                        <ul className="mt-2 max-h-36 space-y-0.5 overflow-y-auto text-[12px] leading-snug text-text-secondary">
                            {progress.logs.slice(-12).map((log, idx) => (
                                <li key={`${log.timestamp_ms}-${idx}`} className="truncate">
                                    {log.message}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden">
                <div className="flex items-center justify-between gap-2">
                    <span className="text-[12px] font-medium text-text">会话日志</span>
                    <Button size="sm" variant="ghost" onClick={onOpenSettingsLog}>
                        在设置中打开完整日志
                    </Button>
                </div>
                <TaskLogPanel startedAt={item.startedAt} enabled={logEnabled} fastPoll />
            </div>
        </div>
    );
};