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
import { CheckCircle2, Loader2, Radio, Repeat, X } from 'lucide-react';
import { Button, Progress } from '../../shared/ui';
import type { HostStatusRow } from '../../core/domain/components/types';
import {
    type ActionProgressView,
    deriveEtaSeconds,
    downloadStageLabel,
    formatBytes,
    formatEta,
    formatSpeed,
    isIndeterminate,
} from '../../core/domain/components/progress';
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

    // 任务三态划分：
    //   - 进行中（pending / running / paused）：UI 显示"取消"按钮
    //   - 终态 linger（success / failed / cancelled）：后端已结束，store 有
    //     3 秒缓冲让用户看清结果。这段时间继续显示"取消"会误导（按了 no-op，
    //     且 ProgressLine 已经写"已完成 / 失败 / 已取消"）。按钮区直接留白，
    //     让文字反馈独立说话；linger 结束 store 自然清掉 activeProgress 之后，
    //     ActionButtons 会重新挂上。
    //   - 无 activeProgress：正常的 ActionButtons
    const isTerminal =
        activeProgress != null &&
        (activeProgress.progress.status === 'success' ||
            activeProgress.progress.status === 'failed' ||
            activeProgress.progress.status === 'cancelled');
    const isCancelable = activeProgress != null && !isTerminal;

    return (
        <div className="relative flex items-center gap-3 overflow-hidden rounded-sm bg-inset/40 px-3 py-2.5 transition-colors hover:bg-inset/70">
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
                {isCancelable ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            onAction({ kind: 'cancel', taskId: activeProgress!.taskId })
                        }
                    >
                        <X size={14} strokeWidth={2} />
                        取消
                    </Button>
                ) : isTerminal ? null : (
                    <ActionButtons
                        row={row}
                        latestRemoteVersion={latestRemoteVersion}
                        onAction={onAction}
                    />
                )}
            </div>
            {/* 进度条绝对定位贴在 row 底部，不挤行高，让卡片之间始终等高。 */}
            {activeProgress && shouldShowProgressBar(activeProgress.progress) && (
                <ProgressBarOverlay progress={activeProgress.progress} />
            )}
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
        return <ProgressLine progress={activeProgress.progress} />;
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
        case 'unknown': {
            // 区分"正在探测"和"真异常"。loading 用中性灰，异常才用 warning 黄。
            const isLoading = row.status.reason === '正在探测';
            return (
                <p
                    className={`mt-0.5 truncate text-[11.5px] ${isLoading ? 'text-text-tertiary' : 'text-warning'
                        }`}
                >
                    {isLoading ? row.status.reason : `探测异常 · ${row.status.reason}`}
                </p>
            );
        }
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

// ─── 进度态行 ─────────────────────────────────────────────────────────────
//
// 三类视觉：
//   1. 下载步骤（downloadStage 非 null + 有字节）：进度条 + 字节/速度/ETA
//   2. 下载步骤但 race / 切镜像（indeterminate）：indeterminate 进度条 + 阶段标签
//   3. 非下载步骤（解压、写文件、检查版本…）：纯文本 + spinner
//
// 切换时尽量保持高度稳定（占用两行），避免 ComponentCard 在装/解压切换时跳动。

const ProgressLine: React.FC<{ progress: ActionProgressView }> = ({ progress }) => {
    // 终态分支放在最前面：失败 / 成功 / 取消。任务在终态会被 store 保留 3 秒
    // 再清掉 active 引用，期间 UI 给明确反馈，避免"点了安装、转一会儿就回到
    // 起点"的迷惑感。完整错误文本走顶部 InfoBar 展示，行内只保留状态标签。
    if (progress.status === 'failed' || progress.status === 'cancelled') {
        const isCancelled = progress.status === 'cancelled';
        return (
            <div className="mt-1 flex items-center gap-2">
                <span
                    aria-hidden
                    className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${isCancelled ? 'bg-warning' : 'bg-danger'
                        }`}
                />
                <span
                    className={`truncate text-[12px] ${isCancelled ? 'text-warning' : 'text-danger'
                        }`}
                >
                    {isCancelled ? '已取消' : '失败 · 详见顶部提示'}
                </span>
            </div>
        );
    }
    if (progress.status === 'success') {
        return (
            <div className="mt-1 flex items-center gap-2">
                <CheckCircle2 size={12} className="shrink-0 text-success" />
                <span className="truncate text-[12px] text-success">已完成</span>
            </div>
        );
    }

    const isDownload = progress.downloadStage != null;
    const indeterminate = isIndeterminate(progress);

    if (!isDownload) {
        return (
            <div className="mt-1 flex items-center gap-2">
                <Loader2 size={12} className="shrink-0 animate-spin text-brand" />
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

    // 单行紧凑布局：图标 · 阶段 · 字节 · 速度 / ETA · 百分比
    // 进度条单独绝对定位贴在 row 底部边缘（HostStatusRowView 渲染），
    // 不让进度态额外占一行行高，避免同行卡片被拉高。
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
                <span className="shrink-0 font-mono tabular-nums text-text-tertiary">
                    {bytesText}
                </span>
            )}
            {trailingMetric && (
                <span className="shrink-0 font-mono tabular-nums text-brand">
                    {trailingMetric}
                </span>
            )}
            <span className="ml-auto shrink-0 font-mono tabular-nums text-text-secondary">
                {indeterminate ? '—' : `${progress.percent}%`}
            </span>
        </div>
    );
};

// 是否要在 row 底部画进度条。失败 / 取消 / 成功 / 非下载步骤都不画。
function shouldShowProgressBar(progress: ActionProgressView): boolean {
    if (progress.status === 'failed' || progress.status === 'cancelled') return false;
    if (progress.status === 'success') return false;
    return true;
}

// 进度条贴 row 底部的覆盖层。h-[2px] 比正常 5px 进度条更克制，因为它不是
// 主视觉信息（数字才是），只是个状态指示。
const ProgressBarOverlay: React.FC<{ progress: ActionProgressView }> = ({ progress }) => {
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
                <Radio
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 animate-pulse text-brand"
                />
            );
        case 'switching_mirror':
            return (
                <Repeat
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 animate-pulse text-warning"
                />
            );
        case 'resuming':
            return (
                <Loader2
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 animate-spin text-info"
                />
            );
        case 'streaming':
        default:
            return (
                <Loader2
                    size={12}
                    strokeWidth={2.2}
                    className="shrink-0 animate-spin text-brand"
                />
            );
    }
};
