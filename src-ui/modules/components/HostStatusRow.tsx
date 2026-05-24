// 组件卡片内部的"单主机一行"。
//
// 职责：纯展示。点击按钮把语义化 action 上抛 onAction，所有 IPC / hook 由
// 父级 ComponentCard 管。
//
// 状态视觉：
//   - installed       绿点 + 版本号 mono + [更新] / [卸载]
//   - not_installed   灰点 + "未安装"      + [安装]
//   - unsupported     横杠 + "不支持当前平台"（按钮禁用，淡灰）
//   - unknown         橙点 + "..."          + [重试探测]
//   - 安装中           进度条 + [取消]      （由父级注入 progress 决定）

import React from 'react';
import { Loader2, X } from 'lucide-react';
import { Button } from '../../shared/ui';
import type { HostStatusRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { StepKind } from '../../core/ipc/types';

export type RowAction =
    | { kind: 'install' }
    | { kind: 'update' }
    | { kind: 'uninstall' }
    | { kind: 'retry_detect' }
    | { kind: 'cancel'; taskId: string };

interface HostStatusRowProps {
    row: HostStatusRow;
    /** 远端最新版本，用来派生"有更新"。null 表示不知道远端版本。 */
    latestRemoteVersion: string | null;
    /** 该 (component, host) 当前活跃的安装/更新 task。null 表示无。 */
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
    onAction: (action: RowAction) => void;
}

export const HostStatusRowView: React.FC<HostStatusRowProps> = ({
    row,
    latestRemoteVersion,
    activeProgress,
    onAction,
}) => {
    const { host, status } = row;

    return (
        <div className="flex items-center gap-3 rounded-sm bg-inset/40 px-3 py-2.5 transition-colors hover:bg-inset/70">
            <StatusDot row={row} hasUpdate={hasUpdate(status, latestRemoteVersion)} />
            <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                    <span className="truncate text-[13px] font-medium text-text">
                        {host.display_name}
                    </span>
                    <span className="shrink-0 text-[10.5px] text-text-tertiary uppercase tracking-wider">
                        {host.os}
                    </span>
                </div>
                <StatusLine
                    row={row}
                    latestRemoteVersion={latestRemoteVersion}
                    activeProgress={activeProgress}
                />
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
                {activeProgress ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => onAction({ kind: 'cancel', taskId: activeProgress.taskId })}
                    >
                        <X size={14} strokeWidth={2} />
                        取消
                    </Button>
                ) : (
                    <ActionButtons
                        row={row}
                        latestRemoteVersion={latestRemoteVersion}
                        onAction={onAction}
                    />
                )}
            </div>
        </div>
    );
};

// ─── helpers ─────────────────────────────────────────────────────────────

function hasUpdate(
    status: HostStatusRow['status'],
    latest: string | null,
): boolean {
    if (status.state !== 'installed' || !latest) return false;
    return status.detected.version !== latest;
}

const StatusDot: React.FC<{
    row: HostStatusRow;
    hasUpdate: boolean;
}> = ({ row, hasUpdate }) => {
    const cls = (() => {
        switch (row.status.state) {
            case 'installed':
                return hasUpdate
                    ? 'bg-warning'
                    : 'bg-success shadow-glow-success';
            case 'not_installed':
                return 'bg-text-disabled';
            case 'unsupported':
                return 'bg-transparent';
            case 'unknown':
                return 'bg-warning';
        }
    })();
    if (row.status.state === 'unsupported') {
        return (
            <span
                aria-hidden
                className="inline-block h-1.5 w-3 shrink-0 rounded-full bg-text-disabled/50"
            />
        );
    }
    return (
        <span
            aria-hidden
            className={`inline-block h-2 w-2 shrink-0 rounded-full ${cls}`}
        />
    );
};

const StatusLine: React.FC<{
    row: HostStatusRow;
    latestRemoteVersion: string | null;
    activeProgress: { taskId: string; progress: ActionProgressView } | null;
}> = ({ row, latestRemoteVersion, activeProgress }) => {
    if (activeProgress) {
        return (
            <div className="mt-1 flex items-center gap-2">
                <Loader2 size={12} className="shrink-0 animate-spin text-brand" />
                <span className="truncate font-mono text-[11.5px] text-text-secondary">
                    {activeProgress.progress.message || '处理中…'}
                </span>
                <span className="shrink-0 font-mono text-[11.5px] tabular-nums text-text-secondary">
                    {activeProgress.progress.percent}%
                </span>
            </div>
        );
    }

    switch (row.status.state) {
        case 'installed': {
            const local = row.status.detected.version;
            const updatable = latestRemoteVersion && local !== latestRemoteVersion;
            return (
                <p className="mt-0.5 truncate font-mono text-[11.5px] text-text-tertiary tabular-nums">
                    {local}
                    {updatable && (
                        <span className="ml-2 text-warning">
                            → {latestRemoteVersion}
                        </span>
                    )}
                </p>
            );
        }
        case 'not_installed':
            return (
                <p className="mt-0.5 truncate text-[11.5px] text-text-tertiary">
                    未安装
                    {latestRemoteVersion && (
                        <span className="ml-2 font-mono tabular-nums">{latestRemoteVersion}</span>
                    )}
                </p>
            );
        case 'unsupported':
            return (
                <p className="mt-0.5 truncate text-[11.5px] text-text-disabled">
                    不支持当前平台
                </p>
            );
        case 'unknown':
            return (
                <p className="mt-0.5 truncate text-[11.5px] text-warning">
                    探测异常 · {row.status.reason}
                </p>
            );
    }
};

const ActionButtons: React.FC<{
    row: HostStatusRow;
    latestRemoteVersion: string | null;
    onAction: (a: RowAction) => void;
}> = ({ row, latestRemoteVersion, onAction }) => {
    switch (row.status.state) {
        case 'installed': {
            const updatable = latestRemoteVersion && row.status.detected.version !== latestRemoteVersion;
            return (
                <>
                    {updatable && (
                        <Button
                            size="sm"
                            variant="primary"
                            onClick={() => onAction({ kind: 'update' })}
                        >
                            更新
                        </Button>
                    )}
                    <Button
                        size="sm"
                        variant="ghost"
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
                    onClick={() => onAction({ kind: 'install' })}
                >
                    安装
                </Button>
            );
        case 'unsupported':
            return null;
        case 'unknown':
            return (
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => onAction({ kind: 'retry_detect' })}
                >
                    重试
                </Button>
            );
    }
};

// 把语义化的 RowAction 翻译成 backend StepKind。在 ComponentCard 调
// startAction 之前用。
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
