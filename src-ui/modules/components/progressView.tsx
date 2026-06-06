// 安装 / 更新进度的行内渲染子件。从 HostStatusRow 抽出来，给机器卡的组件行
// 和旧的 HostStatusRow 共用，避免两份重复的进度 UI。

import React from 'react';
import { CheckCircle2, Loader2, Radio, Repeat } from 'lucide-react';
import { Progress } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import {
    type ActionProgressView,
    deriveEtaSeconds,
    downloadStageLabel,
    formatBytes,
    formatEta,
    formatSpeed,
    isIndeterminate,
} from '../../core/domain/components/progress';

function progressSuccessEnterKey(progress: ActionProgressView): string {
    return `success-${progress.percent}-${progress.message ?? ''}`;
}

export const ProgressLine: React.FC<{ progress: ActionProgressView }> = ({ progress }) => {
    if (progress.status === 'failed' || progress.status === 'cancelled') {
        const isCancelled = progress.status === 'cancelled';
        return (
            <div className="mt-1 flex items-center gap-2">
                <span
                    aria-hidden
                    className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${isCancelled ? 'bg-warning' : 'bg-danger'}`}
                />
                <span className={`truncate text-[12px] ${isCancelled ? 'text-warning' : 'text-danger'}`}>
                    {isCancelled ? '已取消' : '失败 · 详见顶部提示'}
                </span>
            </div>
        );
    }
    if (progress.status === 'success') {
        return (
            <div className="mt-1 flex items-center gap-2">
                <MotionIcon
                    icon={CheckCircle2}
                    motion="pulse"
                    playEnter
                    enterKey={progressSuccessEnterKey(progress)}
                    size={12}
                    className="shrink-0 text-success"
                />
                <span className="truncate text-[12px] text-success">已完成</span>
            </div>
        );
    }

    const isDownload = progress.downloadStage != null;
    const indeterminate = isIndeterminate(progress);

    if (!isDownload) {
        return (
            <div className="mt-1 flex items-center gap-2">
                <MotionIcon
                    icon={Loader2}
                    motion="spin"
                    playEnter={false}
                    size={12}
                    className="shrink-0 text-brand"
                />
                <span className="truncate text-[12px] text-text-secondary">
                    {progress.message || '处理中…'}
                </span>
                <span className="ml-auto shrink-0 font-mono text-[11.5px] tabular-nums text-text-secondary">
                    {progress.percent}%
                </span>
            </div>
        );
    }

    const stageLabel = downloadStageLabel(progress.downloadStage) ?? '下载中';
    const eta = deriveEtaSeconds(progress);
    const bytesText =
        progress.downloadedBytes != null && progress.totalBytes != null
            ? `${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`
            : progress.downloadedBytes != null
              ? formatBytes(progress.downloadedBytes)
              : null;
    const trailingMetric =
        progress.speedBps != null
            ? formatSpeed(progress.speedBps)
            : eta != null
              ? `ETA ${formatEta(eta)}`
              : null;

    return (
        <div className="mt-1 flex items-center gap-2 text-[11.5px]">
            <StageIcon stage={progress.downloadStage} />
            <span className="truncate text-text-secondary">{stageLabel}</span>
            {bytesText && (
                <span className="shrink-0 font-mono tabular-nums text-text-tertiary">{bytesText}</span>
            )}
            {trailingMetric && (
                <span className="shrink-0 font-mono tabular-nums text-brand">{trailingMetric}</span>
            )}
            <span className="ml-auto shrink-0 font-mono tabular-nums text-text-secondary">
                {indeterminate ? '—' : `${progress.percent}%`}
            </span>
        </div>
    );
};

export function shouldShowProgressBar(progress: ActionProgressView): boolean {
    if (progress.status === 'failed' || progress.status === 'cancelled') return false;
    if (progress.status === 'success') return false;
    return true;
}

export const ProgressBarOverlay: React.FC<{ progress: ActionProgressView }> = ({ progress }) => {
    const indeterminate = isIndeterminate(progress);
    const tone = progress.downloadStage === 'switching_mirror' ? 'warning' : 'brand';
    const isDownload = progress.downloadStage != null;
    return (
        <Progress
            size="sm"
            tone={tone}
            value={progress.percent}
            indeterminate={indeterminate || !isDownload}
            className="absolute inset-x-0 bottom-0 h-[2px] rounded-none bg-transparent"
        />
    );
};

const StageIcon: React.FC<{ stage: ActionProgressView['downloadStage'] }> = ({ stage }) => {
    switch (stage) {
        case 'racing':
            return (
                <MotionIcon
                    icon={Radio}
                    motion="pulse"
                    playEnter={false}
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 text-brand"
                />
            );
        case 'switching_mirror':
            return (
                <MotionIcon
                    icon={Repeat}
                    motion="pulse"
                    playEnter={false}
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 text-warning"
                />
            );
        case 'resuming':
            return (
                <MotionIcon
                    icon={Loader2}
                    motion="spin"
                    playEnter={false}
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 text-info"
                />
            );
        case 'streaming':
        default:
            return (
                <MotionIcon
                    icon={Loader2}
                    motion="spin"
                    playEnter={false}
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 text-brand"
                />
            );
    }
};