// Docker 引擎的行内状态、环境操作与安装进度。
import React from 'react';
import {
    ChevronDown,
    Container,
    Copy,
    Download,
    ExternalLink,
    Loader2,
    RefreshCw,
} from 'lucide-react';
import {
    Button,
    Badge,
    ContextMenu,
    ContextMenuTrigger,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    Popover,
    PopoverTrigger,
    PopoverContent,
} from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import { dockerSetupAction } from '../../core/domain/docker/setup';
import { errorText } from '../../core/domain/errors';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DockerStatus, Os } from '../../core/ipc/types';
import { shouldShowProgressBar, ProgressBarOverlay } from '../../shared/components/progressView';
import { dockerRowStatusBadge, type StatusBadgeSpec } from './componentStatusPresentation';

interface DockerRowProps {
    os: Os;
    status: DockerStatus | undefined;
    isProbing: boolean;
    probeError?: unknown;
    isInstalling: boolean;
    installHint?: string;
    installProgress?: ActionProgressView | null;
    onInstall: () => void;
    onOpenDownload: () => void;
    onRetry?: () => void;
}

export const DockerRow: React.FC<DockerRowProps> = ({
    os,
    status,
    isProbing,
    probeError,
    isInstalling,
    installHint,
    installProgress,
    onInstall,
    onOpenDownload,
    onRetry,
}) => {
    const openExternal = useOpenExternal();
    const summary = status ? dockerStatusSummary(status) : null;
    const failed = probeError != null;
    const ready = !failed && (summary?.ready ?? false);
    const autoInstallable = os === 'linux';
    const probing = isProbing;
    const action = dockerSetupAction(status);
    const canSetup = !failed && !isProbing && !!status && !ready;
    const statusBadge: StatusBadgeSpec = isInstalling
        ? { tone: 'brand' as const, label: action?.pendingLabel ?? '处理中' }
        : failed
          ? { tone: 'danger' as const, label: '探测失败' }
          : probing
            ? { tone: 'neutral' as const, label: '检测中' }
            : !status && !probing
              ? { tone: 'neutral' as const, label: '待探测' }
              : status?.installed && !ready && !probing
                ? { tone: 'warning' as const, label: status.daemonRunning ? '待补全' : '未启动' }
                : dockerRowStatusBadge({ ready, probing, inFlight: false });

    const footer = failed ? (
        <Button size="sm" variant="secondary" onClick={onRetry} disabled={isProbing || !onRetry}>
            <RefreshCw size={13} />
            重新探测
        </Button>
    ) : canSetup && autoInstallable ? (
        <Button size="sm" variant="primary" onClick={onInstall} disabled={isInstalling}>
            {isInstalling && (
                <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
            )}
            {isInstalling ? action?.pendingLabel : action?.label}
        </Button>
    ) : canSetup ? (
        <Button size="sm" variant="secondary" onClick={onOpenDownload}>
            去官网安装
        </Button>
    ) : null;

    const handleCopyVersion = async (v: string) => {
        try {
            await navigator.clipboard.writeText(v);
            pushInfoBar({
                tone: 'info',
                title: '已复制 Docker 版本号',
                content: v,
                autoDismissMs: 2000,
            });
        } catch {
            // ignore
        }
    };

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild>
                <div className="relative min-w-0 overflow-hidden border-b border-border-subtle bg-inset/20 px-4 py-3">
                    <div className="grid min-h-11 grid-cols-[minmax(0,1fr)_auto] items-center gap-3">
                        <div className="flex min-w-0 items-center gap-3">
                            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-info-soft text-info">
                                <Container size={21} strokeWidth={1.8} />
                            </span>
                            <div className="flex h-11 min-w-0 flex-col items-start justify-center gap-1 @min-[24rem]:flex-row @min-[24rem]:items-center @min-[24rem]:justify-start @min-[24rem]:gap-2">
                                <Popover>
                                    <h4 className="min-w-0 max-w-full font-display text-sm font-semibold text-text">
                                        <PopoverTrigger asChild>
                                            <button
                                                type="button"
                                                aria-label="Docker 环境详情"
                                                className="flex max-w-full items-center gap-1 rounded-sm text-left hover:text-brand focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                            >
                                                <span className="truncate">Docker 引擎</span>
                                                <ChevronDown
                                                    size={13}
                                                    className="shrink-0 text-text-tertiary"
                                                />
                                            </button>
                                        </PopoverTrigger>
                                    </h4>
                                    <PopoverContent
                                        align="start"
                                        aria-label="Docker 环境详情"
                                        className="max-h-[var(--radix-popover-content-available-height)] w-80 max-w-[calc(100vw-2rem)] overflow-y-auto p-4"
                                    >
                                        <div className="mb-3 flex items-center justify-between gap-3">
                                            <p className="text-sm font-semibold text-text">
                                                Docker 环境
                                            </p>
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                onClick={() =>
                                                    openExternal('https://www.docker.com/')
                                                }
                                            >
                                                <ExternalLink size={13} />
                                                官网
                                            </Button>
                                        </div>
                                        <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-3 text-sm">
                                            <dt className="text-text-secondary">引擎版本</dt>
                                            <dd className="min-w-0 break-all text-text">
                                                {status?.installed
                                                    ? status.version || '未知'
                                                    : status
                                                      ? '未安装'
                                                      : '待检测'}
                                            </dd>
                                            <dt className="text-text-secondary">Compose</dt>
                                            <dd className="text-text">
                                                {status?.composeAvailable
                                                    ? '已就绪'
                                                    : status
                                                      ? '未就绪'
                                                      : '待检测'}
                                            </dd>
                                        </dl>
                                        {probeError != null ? (
                                            <p
                                                role="status"
                                                className="mt-3 break-words text-sm text-danger"
                                            >
                                                {errorText(probeError)}
                                            </p>
                                        ) : isInstalling ? (
                                            <p
                                                role="status"
                                                className="mt-3 break-words text-sm text-text-secondary"
                                            >
                                                {installProgress?.message ||
                                                    installHint ||
                                                    '正在准备 Docker 环境'}
                                            </p>
                                        ) : null}
                                    </PopoverContent>
                                </Popover>
                                <Badge
                                    tone={statusBadge.tone}
                                    appearance="soft"
                                    dot={statusBadge.dot}
                                    className="max-w-full shrink-0 whitespace-nowrap text-xs"
                                >
                                    {statusBadge.label}
                                </Badge>
                            </div>
                        </div>
                        <div className="flex h-7 shrink-0 items-center whitespace-nowrap">
                            {footer}
                        </div>
                    </div>
                    {isInstalling && installProgress && shouldShowProgressBar(installProgress) ? (
                        <ProgressBarOverlay progress={installProgress} determinate />
                    ) : null}
                </div>
            </ContextMenuTrigger>

            <ContextMenuContent className="w-52">
                <ContextMenuLabel className="font-mono text-2xs truncate">
                    Docker 运行环境
                </ContextMenuLabel>
                <ContextMenuSeparator />

                {canSetup && autoInstallable && (
                    <ContextMenuItem tone="brand" disabled={isInstalling} onClick={onInstall}>
                        <Download size={13} className="text-brand" />
                        <span>{isInstalling ? action?.pendingLabel : action?.label}</span>
                    </ContextMenuItem>
                )}

                {canSetup && !autoInstallable && (
                    <ContextMenuItem onClick={onOpenDownload}>
                        <ExternalLink size={13} />
                        <span>去官网下载安装</span>
                    </ContextMenuItem>
                )}

                <ContextMenuItem onClick={() => openExternal('https://www.docker.com/')}>
                    <ExternalLink size={13} />
                    <span>访问 Docker 官网</span>
                </ContextMenuItem>

                {ready && status?.version && (
                    <>
                        <ContextMenuSeparator />
                        <ContextMenuItem onClick={() => handleCopyVersion(status.version!)}>
                            <Copy size={13} />
                            <span>复制 Docker 版本 ({status.version})</span>
                        </ContextMenuItem>
                    </>
                )}
            </ContextMenuContent>
        </ContextMenu>
    );
};

export default DockerRow;
