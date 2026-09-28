// 实例「安装中」时的进度：当前步骤 + 百分比一行，外加一条进度条。
// 卡片版的细条贴在卡片底边（绝对定位，找最近的定位祖先，和组件页卡片一个位置）；详情版是普通条。

import React from 'react';
import { Progress } from '../../shared/ui';
import { useAppInstallProgress } from '../../hooks/apps/useAppInstallProgress';
import type { ActionProgressView } from '../../core/domain/components/progress';
import { isIndeterminate } from '../../core/domain/components/progress';
import { ProgressBarOverlay, ProgressLine, shouldShowProgressBar } from '../../shared/components/progressView';
import type { AppInstance } from '../../core/ipc/types';

/** 还在排队时进度里没有步骤文案，给一句人话 */
function withQueueHint(progress: ActionProgressView): ActionProgressView {
    if (progress.status === 'pending' && !progress.message.trim()) {
        return { ...progress, message: '排队中，等前面的任务' };
    }
    return progress;
}

export const CardInstallProgress: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const progress = useAppInstallProgress(instance);
    if (!progress) {
        return <p className="truncate text-text-tertiary">正在准备安装…</p>;
    }
    return (
        <>
            <ProgressLine progress={withQueueHint(progress)} className="mt-0" />
            {shouldShowProgressBar(progress) && <ProgressBarOverlay progress={progress} />}
        </>
    );
};

export const DetailInstallProgress: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const progress = useAppInstallProgress(instance);
    return (
        <div className="flex w-full max-w-sm flex-col gap-2">
            {progress ? (
                <>
                    <ProgressLine progress={withQueueHint(progress)} className="mt-0" />
                    {shouldShowProgressBar(progress) && (
                        <Progress
                            size="sm"
                            tone="brand"
                            value={progress.overallPercent}
                            indeterminate={isIndeterminate(progress) || progress.overallPercent <= 0}
                        />
                    )}
                </>
            ) : (
                <p className="text-xs text-text-tertiary">正在准备安装…</p>
            )}
        </div>
    );
};
