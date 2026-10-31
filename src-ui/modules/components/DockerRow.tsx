// Docker 引擎的行内状态、环境操作与安装进度。
import React from 'react';
import { Info, Container, Copy, Download, ExternalLink, Loader2, RefreshCw } from 'lucide-react';
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
    engine?: 'docker' | 'podman';
    engineControl?: React.ReactNode;
    probeLabel?: string;
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
    engine = 'docker',
    engineControl,
    probeLabel,
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
    const engineName = engine === 'podman' ? 'Podman' : 'Docker';
    const summary = status ? dockerStatusSummary(status, engineName) : null;
    const failed = probeError != null;
    const ready = !failed && (summary?.ready ?? false);
    const autoInstallable = os === 'linux';
    const probing = isProbing;
    const setup = dockerSetupAction(status);
    const action =
        engine === 'podman' && status?.installed && !status.daemonRunning
            ? { label: '检查 Podman 环境', pendingLabel: '正在检查' }
            : setup && { ...setup, label: setup.label.replace('Docker', engineName) };
    const canSetup = !failed && !isProbing && !!status && !ready;
    const statusBadge: StatusBadgeSpec = isInstalling
        ? { tone: 'brand' as const, label: action?.pendingLabel ?? '处理中' }
        : failed
          ? { tone: 'danger' as const, label: '探测失败' }
          : probing
            ? { tone: 'neutral' as const, label: probeLabel ?? '检测中' }
            : !status && !probing
              ? { tone: 'neutral' as const, label: '待探测' }
              : status?.installed && !ready && !probing
                ? {
                      tone: 'warning' as const,
                      label: status.daemonRunning
                          ? '待补全'
                          : engine === 'podman'
                            ? '环境异常'
                            : '未启动',
                  }
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
                title: `已复制 ${engineName} 版本号`,
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
                    <div className="grid min-h-11 grid-cols-1 items-start gap-3 @min-[30rem]:grid-cols-[minmax(0,1fr)_auto] @min-[30rem]:items-center">
                        <div className="flex min-w-0 items-center gap-3">
                            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-info-soft text-info">
                                <Container size={21} strokeWidth={1.8} />
                            </span>
                            <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                                <div className="flex flex-wrap items-center gap-2">
                                    {engineControl ?? (
                                        <h4 className="text-sm font-semibold text-text">
                                            {engineName} 引擎
                                        </h4>
                                    )}
                                    <Popover>
                                        <PopoverTrigger asChild>
                                            <button
                                                type="button"
                                                aria-label={`${engineName} 环境详情`}
                                                disabled={!!probeLabel}
                                                className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary hover:bg-field hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                            >
                                                <Info size={15} />
                                            </button>
                                        </PopoverTrigger>
                                        <PopoverContent
                                            align="start"
                                            aria-label={`${engineName} 环境详情`}
                                            className="max-h-[var(--radix-popover-content-available-height)] w-80 max-w-[calc(100vw-2rem)] overflow-y-auto p-4"
                                        >
                                            <div className="mb-3 flex items-center justify-between gap-3">
                                                <p className="text-sm font-semibold text-text">
                                                    {engineName} 环境
                                                </p>
                                                <Button
                                                    size="sm"
                                                    variant="ghost"
                                                    onClick={() =>
                                                        openExternal(
                                                            engine === 'podman'
                                                                ? 'https://podman.io/'
                                                                : 'https://www.docker.com/',
                                                        )
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
                                                        `正在准备 ${engineName} 环境`}
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
                                {engineControl ? (
                                    <p className="text-xs leading-relaxed text-text-secondary">
                                        容器和镜像按引擎分别管理
                                    </p>
                                ) : null}
                            </div>
                        </div>
                        <div className="flex h-7 shrink-0 items-center justify-end whitespace-nowrap">
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
                    {engineName} 运行环境
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

                <ContextMenuItem
                    onClick={() =>
                        openExternal(
                            engine === 'podman' ? 'https://podman.io/' : 'https://www.docker.com/',
                        )
                    }
                >
                    <ExternalLink size={13} />
                    <span>访问 {engineName} 官网</span>
                </ContextMenuItem>

                {ready && status?.version && (
                    <>
                        <ContextMenuSeparator />
                        <ContextMenuItem onClick={() => handleCopyVersion(status.version!)}>
                            <Copy size={13} />
                            <span>
                                复制 {engineName} 版本 ({status.version})
                            </span>
                        </ContextMenuItem>
                    </>
                )}
            </ContextMenuContent>
        </ContextMenu>
    );
};

export default DockerRow;
