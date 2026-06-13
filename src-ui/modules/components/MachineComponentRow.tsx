// 机器卡里的一行：一个组件在这台机器上的状态 + 操作。
//
// 跟旧的 HostStatusRow 视觉一致，区别是主标题用"组件名"（因为现在卡片以机器
// 为单位，行里要区分的是组件而不是主机）。进度渲染直接复用旧的子件。

import React from 'react';
import { X } from 'lucide-react';
import { Button } from '../../shared/ui';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import type { MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import { ProgressLine, ProgressBarOverlay, shouldShowProgressBar } from './progressView';
import { ComponentCardBody, ComponentEntityCard } from './ComponentEntityCard';
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
    /// 可选尾随动作（如框架行的「Docker 部署」按钮）。与常规安装/卸载按钮并排，
    /// 仅在没有正在跑的 action 时显示——跑安装时只留取消按钮，不让用户分心。
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

    const isTerminal =
        activeProgress != null &&
        (activeProgress.progress.status === 'success' ||
            activeProgress.progress.status === 'failed' ||
            activeProgress.progress.status === 'cancelled');
    const isCancelable = activeProgress != null && !isTerminal;

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
    ) : isTerminal ? undefined : (
        <div className="flex min-w-0 max-w-full flex-wrap items-center justify-end gap-1.5">
            {trailingActions}
            <ActionButtons
                status={status}
                latestRemoteVersion={latestRemoteVersion}
                disabled={isAnyInstalling}
                onAction={handle}
            />
        </div>
    );

    return (
        <ComponentEntityCard
            footer={footer}
            progressOverlay={
                showProgressBar ? (
                    <ProgressBarOverlay progress={activeProgress!.progress} />
                ) : undefined
            }
        >
            <ComponentCardBody
                statusDot={
                    <StatusDot
                        status={status}
                        hasUpdate={hasUpdate(status, latestRemoteVersion)}
                        inFlight={inFlight}
                    />
                }
                titleRow={
                    <div className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 flex-1 truncate font-display text-md font-semibold leading-tight text-text">
                            {info.display_name}
                        </span>
                        {info.repo_url && (
                            <a
                                href={info.repo_url}
                                onClick={(e) => {
                                    e.preventDefault();
                                    openExternal(info.repo_url!);
                                }}
                                className="ml-auto shrink-0 cursor-pointer text-2xs text-text-tertiary hover:text-text"
                            >
                                仓库
                            </a>
                        )}
                    </div>
                }
                description={info.description || undefined}
                statusLine={
                    <StatusLine
                        status={status}
                        latestRemoteVersion={latestRemoteVersion}
                        activeProgress={activeProgress}
                    />
                }
            />
        </ComponentEntityCard>
    );
};

function hasUpdate(status: MachineComponentRow['status'], latest: string | null): boolean {
    if (status.state !== 'installed' || !latest) return false;
    return status.detected.version !== latest;
}

const StatusDot: React.FC<{
    status: MachineComponentRow['status'];
    hasUpdate: boolean;
    inFlight: boolean;
}> = ({ status, hasUpdate, inFlight }) => {
    if (inFlight) {
        return (
            <span
                aria-hidden
                className="mt-1.5 inline-block h-2.5 w-2.5 shrink-0 rounded-full bg-brand shadow-glow-brand"
            />
        );
    }
    const cls = (() => {
        switch (status.state) {
            case 'installed':
                return hasUpdate ? 'bg-warning' : 'bg-success shadow-glow-success';
            case 'not_installed':
                return 'bg-text-disabled';
            case 'unsupported':
                return 'bg-text-disabled/50';
            case 'unknown':
                return 'bg-warning';
        }
    })();
    return <span aria-hidden className={`mt-1.5 inline-block h-2.5 w-2.5 shrink-0 rounded-full ${cls}`} />;
};

const StatusLine: React.FC<{
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
                <p className="mt-0 truncate font-mono text-xs tabular-nums text-text-tertiary">
                    {local}
                    {updatable && <span className="ml-2 text-warning">→ {latestRemoteVersion}</span>}
                </p>
            );
        }
        case 'not_installed':
            return latestRemoteVersion ? (
                <p className="mt-0 truncate font-mono text-xs tabular-nums text-text-tertiary">
                    {latestRemoteVersion}
                </p>
            ) : null;
        case 'unsupported':
            return (
                <p className="mt-0 truncate text-xs text-text-disabled">不支持当前平台</p>
            );
        case 'unknown': {
            const isLoading = status.reason === '正在探测';
            if (isLoading) {
                return (
                    <p className="mt-0 truncate text-[12px] text-text-tertiary">正在探测</p>
                );
            }
            // 探测失败：原因可能是后端拼的长句（"自动连接被拒绝…请去远端页手动
            // 测试"）。truncate 会切没，改成最多两行 + title 兜底，让用户读得到
            // 真因而不是只看到"探测异常"。
            return (
                <p
                    title={status.reason}
                    className="mt-0 line-clamp-2 text-xs text-warning"
                >
                    探测异常 · {status.reason}
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
                        <Button size="sm" variant="primary" disabled={disabled} onClick={() => onAction({ kind: 'update' })}>
                            更新
                        </Button>
                    )}
                    <Button size="sm" variant="ghost" disabled={disabled} onClick={() => onAction({ kind: 'uninstall' })}>
                        卸载
                    </Button>
                </>
            );
        }
        case 'not_installed':
            return (
                <Button size="sm" variant="primary" disabled={disabled} onClick={() => onAction({ kind: 'install' })}>
                    安装
                </Button>
            );
        case 'unsupported':
            return null;
        case 'unknown':
            return (
                <Button size="sm" variant="secondary" disabled={disabled} onClick={() => onAction({ kind: 'retry_detect' })}>
                    重试
                </Button>
            );
    }
};
