// 机器卡里的一行：一个组件在这台机器上的状态 + 操作。

import React, { useEffect, useState } from 'react';
import { ExternalLink, ScrollText, X } from 'lucide-react';
import { Button } from '../../shared/ui';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import type { MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import {
    compareSemver,
    type ReleaseInfoView,
} from '../../core/domain/release/normalize';
import { ProgressLine, ProgressBarOverlay, shouldShowProgressBar } from './progressView';
import { ComponentManageCard } from './ComponentEntityCard';
import { hostComponentStatusBadge } from './componentStatusPresentation';
import type { StepKind } from '../../core/ipc/types';

export type RowAction =
    | { kind: 'install' }
    | { kind: 'update' }
    | { kind: 'uninstall' }
    | { kind: 'retry_detect' }
    | { kind: 'cancel'; taskId: string };

export function rowActionToStepKind(action: RowAction): StepKind | null {
    switch (action.kind) {
        case 'install':
            return 'ensure_installed';
        case 'update':
            return 'update';
        case 'uninstall':
            return 'uninstall';
        default:
            return null;
    }
}

interface Props {
    row: MachineComponentRow;
    hostId: string;
    latestRemoteVersion: string | null;
    /** 远端 release 元数据（含更新日志）；null 表示尚未拉到 */
    latestRelease?: ReleaseInfoView | null;
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
    disabled?: boolean;
    /** 本机有活跃 Bot 时限制 update/uninstall；安装仍可用 */
    lifecycleBlockedReason?: string | null;
    onAction: (action: { stepKind: StepKind } | { cancelTaskId: string }) => void;
    onRetryDetect: () => void;
    onShowReleaseNotes?: () => void;
    trailingActions?: React.ReactNode;
}

export const MachineComponentRowView: React.FC<Props> = ({
    row,
    latestRemoteVersion,
    latestRelease = null,
    activeProgress,
    disabled = false,
    lifecycleBlockedReason = null,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    trailingActions,
}) => {
    const { info, status } = row;
    const openExternal = useOpenExternal();

    const isCancelable =
        activeProgress != null &&
        activeProgress.progress.status !== 'success' &&
        activeProgress.progress.status !== 'failed' &&
        activeProgress.progress.status !== 'cancelled';

    const handle = (action: RowAction) => {
        if (action.kind === 'retry_detect') {
            onRetryDetect();
            return;
        }
        if (action.kind === 'cancel') {
            onAction({ cancelTaskId: action.taskId });
            return;
        }
        const stepKind = rowActionToStepKind(action);
        if (stepKind) onAction({ stepKind });
    };

    const showProgressBar =
        activeProgress && shouldShowProgressBar(activeProgress.progress);

    const inFlight =
        activeProgress != null &&
        activeProgress.progress.status !== 'success' &&
        activeProgress.progress.status !== 'failed' &&
        activeProgress.progress.status !== 'cancelled';

    const canShowNotes = !!latestRelease && !!onShowReleaseNotes;

    const footer = isCancelable ? (
        <Button
            size="sm"
            variant="ghost"
            onClick={() => handle({ kind: 'cancel', taskId: activeProgress!.taskId })}
        >
            <X size={14} strokeWidth={2} />
            取消
        </Button>
    ) : (
        <>
            {canShowNotes ? (
                <Button
                    size="sm"
                    variant="ghost"
                    title="查看更新日志"
                    onClick={onShowReleaseNotes}
                >
                    <ScrollText size={13} strokeWidth={2} aria-hidden />
                    日志
                </Button>
            ) : null}
            {trailingActions}
            <ActionButtons
                status={status}
                latestRemoteVersion={latestRemoteVersion}
                disabled={disabled}
                lifecycleBlockedReason={lifecycleBlockedReason}
                onAction={handle}
            />
        </>
    );

    const titleAside = info.repo_url ? (
        <button
            type="button"
            onClick={() => openExternal(info.repo_url!)}
            className="inline-flex items-center gap-0.5 text-2xs text-text-tertiary transition-colors hover:text-brand"
        >
            仓库
            <ExternalLink size={11} strokeWidth={2} aria-hidden />
        </button>
    ) : undefined;

    return (
        <ComponentManageCard
            accent={inFlight ? 'brand' : 'none'}
            statusBadge={hostComponentStatusBadge(status, {
                hasUpdate: hasUpdate(status, latestRemoteVersion),
                inFlight,
            })}
            title={info.display_name}
            titleAside={titleAside}
            description={info.description || undefined}
            meta={
                <StatusMeta
                    status={status}
                    latestRemoteVersion={latestRemoteVersion}
                    activeProgress={activeProgress}
                />
            }
            footer={footer}
            progressOverlay={
                showProgressBar ? (
                    <ProgressBarOverlay progress={activeProgress!.progress} />
                ) : undefined
            }
        />
    );
};

function hasUpdate(status: MachineComponentRow['status'], latest: string | null): boolean {
    if (status.state !== 'installed' || !latest) return false;
    return compareSemver(status.detected.version, latest) > 0;
}

const TERMINAL_DISMISS_MS = 3500;

function isTerminalStatus(status: ActionProgressView['status']): boolean {
    return status === 'success' || status === 'failed' || status === 'cancelled';
}

/**
 * 终态进度行短留后自动让位给本地版本号。
 *
 * 终态（success/failed/cancelled）保留 {@link TERMINAL_DISMISS_MS} 让用户看清
 * 「已完成」反馈，到点后回退到本地版本分支。进行中状态始终显示进度行。
 *
 * 用 taskId + status 做 key：换任务或同一任务从进行中切到终态时重置计时器，
 * 确保终态反馈恰好显示固定时长。
 */
function useTerminalDismiss(
    activeProgress: { taskId: string; progress: ActionProgressView } | null,
): boolean {
    const [dismissed, setDismissed] = useState(false);

    const terminal =
        activeProgress != null && isTerminalStatus(activeProgress.progress.status);
    // 终态的稳定性 key：task + status 都不变才视为「同一个终态」，重置会触发计时器重启。
    const terminalKey =
        activeProgress != null && terminal
            ? `${activeProgress.taskId}::${activeProgress.progress.status}`
            : null;

    useEffect(() => {
        // 进行中（含 null）重置 dismissed，让下一次终态重新计时。
        if (!terminal) {
            setDismissed(false);
            return;
        }
        setDismissed(false);
        const timer = setTimeout(() => setDismissed(true), TERMINAL_DISMISS_MS);
        return () => clearTimeout(timer);
    }, [terminal, terminalKey]);

    // 只在「终态且已过 dismissed 计时」时不显示进度行；其它情况（进行中 / 终态短留窗口内）都显示。
    return terminal && dismissed;
}

const StatusMeta: React.FC<{
    status: MachineComponentRow['status'];
    latestRemoteVersion: string | null;
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
}> = ({ status, latestRemoteVersion, activeProgress }) => {
    const dismissProgress = useTerminalDismiss(activeProgress);
    if (activeProgress && !dismissProgress) {
        return <ProgressLine progress={activeProgress.progress} className="mt-0" />;
    }
    switch (status.state) {
        case 'installed': {
            const local = status.detected.version;
            const updatable =
                !!latestRemoteVersion && compareSemver(local, latestRemoteVersion) > 0;
            return (
                <p className="truncate font-mono text-xs tabular-nums text-text-tertiary">
                    本地 {local}
                    {updatable && (
                        <span className="ml-1.5 text-warning">最新 {latestRemoteVersion}</span>
                    )}
                </p>
            );
        }
        case 'not_installed':
            return latestRemoteVersion ? (
                <p className="truncate font-mono text-xs tabular-nums text-text-tertiary">
                    可装版本 {latestRemoteVersion}
                </p>
            ) : (
                <p className="truncate text-xs text-text-disabled">暂无远端版本信息</p>
            );
        case 'unsupported':
            return (
                <p className="truncate text-xs text-text-disabled">当前系统不支持此组件</p>
            );
        case 'unknown': {
            if (status.reason === '正在探测') {
                return <p className="truncate text-xs text-text-tertiary">正在探测安装状态…</p>;
            }
            return (
                <p className="truncate text-xs text-text-tertiary">探测未成功，详见顶部提示</p>
            );
        }
    }
};

const ActionButtons: React.FC<{
    status: MachineComponentRow['status'];
    latestRemoteVersion: string | null;
    disabled?: boolean;
    lifecycleBlockedReason?: string | null;
    onAction: (a: RowAction) => void;
}> = ({ status, latestRemoteVersion, disabled, lifecycleBlockedReason, onAction }) => {
    const lifecycleDisabled = disabled || !!lifecycleBlockedReason;
    switch (status.state) {
        case 'installed': {
            const updatable =
                !!latestRemoteVersion &&
                compareSemver(status.detected.version, latestRemoteVersion) > 0;
            return (
                <>
                    {updatable && (
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={lifecycleDisabled}
                            title={lifecycleBlockedReason ?? undefined}
                            onClick={() => onAction({ kind: 'update' })}
                        >
                            更新
                        </Button>
                    )}
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={lifecycleDisabled}
                        title={lifecycleBlockedReason ?? undefined}
                        onClick={() => onAction({ kind: 'uninstall' })}
                    >
                        卸载
                    </Button>
                </>
            );
        }
        case 'not_installed':
            return (
                <Button
                    size="sm"
                    variant="primary"
                    disabled={disabled}
                    onClick={() => onAction({ kind: 'install' })}
                >
                    安装
                </Button>
            );
        case 'unsupported':
            return <span className="text-2xs text-text-disabled">无可用操作</span>;
        case 'unknown':
            return (
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={disabled}
                    onClick={() => onAction({ kind: 'retry_detect' })}
                >
                    重新探测
                </Button>
            );
    }
};
