// 机器卡里的一行：一个组件在这台机器上的状态 + 操作。

import React from 'react';
import { ExternalLink, X } from 'lucide-react';
import { Button } from '../../shared/ui';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import type { MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
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
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
    isAnyInstalling: boolean;
    onAction: (action: { stepKind: StepKind } | { cancelTaskId: string }) => void;
    onRetryDetect: () => void;
    trailingActions?: React.ReactNode;
}

export const MachineComponentRowView: React.FC<Props> = ({
    row,
    latestRemoteVersion,
    activeProgress,
    isAnyInstalling,
    onAction,
    onRetryDetect,
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
            {trailingActions}
            <ActionButtons
                status={status}
                latestRemoteVersion={latestRemoteVersion}
                disabled={isAnyInstalling}
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
    return status.detected.version !== latest;
}

const StatusMeta: React.FC<{
    status: MachineComponentRow['status'];
    latestRemoteVersion: string | null;
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
}> = ({ status, latestRemoteVersion, activeProgress }) => {
    if (activeProgress) {
        return <ProgressLine progress={activeProgress.progress} className="mt-0" />;
    }
    switch (status.state) {
        case 'installed': {
            const local = status.detected.version;
            const updatable = latestRemoteVersion && local !== latestRemoteVersion;
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
                <p title={status.reason} className="truncate text-xs text-warning">
                    {status.reason}
                </p>
            );
        }
    }
};

const ActionButtons: React.FC<{
    status: MachineComponentRow['status'];
    latestRemoteVersion: string | null;
    disabled?: boolean;
    onAction: (a: RowAction) => void;
}> = ({ status, latestRemoteVersion, disabled, onAction }) => {
    switch (status.state) {
        case 'installed': {
            const updatable =
                latestRemoteVersion && status.detected.version !== latestRemoteVersion;
            return (
                <>
                    {updatable && (
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={disabled}
                            onClick={() => onAction({ kind: 'update' })}
                        >
                            更新
                        </Button>
                    )}
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={disabled}
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