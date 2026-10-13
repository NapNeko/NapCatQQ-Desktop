import React from 'react';
import { Container, Copy, Download, ExternalLink, Loader2 } from 'lucide-react';
import {
    Button,
    ContextMenu,
    ContextMenuTrigger,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
} from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DockerStatus, Os } from '../../core/ipc/types';
import { shouldShowProgressBar, ProgressBarOverlay } from '../../shared/components/progressView';
import { ComponentManageCard } from './ComponentEntityCard';
import { dockerRowStatusBadge } from './componentStatusPresentation';

interface DockerRowProps {
    os: Os;
    status: DockerStatus | undefined;
    isProbing: boolean;
    isInstalling: boolean;
    installHint?: string;
    installProgress?: ActionProgressView | null;
    onInstall: () => void;
    onOpenDownload: () => void;
}

export const DockerRow: React.FC<DockerRowProps> = ({
    os,
    status,
    isProbing,
    isInstalling,
    installHint,
    installProgress,
    onInstall,
    onOpenDownload,
}) => {
    const openExternal = useOpenExternal();
    const summary = status ? dockerStatusSummary(status) : null;
    const ready = summary?.ready ?? false;
    const autoInstallable = os === 'linux';
    const probing = isProbing && !status;

    const footer =
        ready || probing ? null : autoInstallable ? (
            <Button size="sm" variant="primary" onClick={onInstall} disabled={isInstalling}>
                {isInstalling && (
                    <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
                )}
                安装
            </Button>
        ) : (
            <Button size="sm" variant="secondary" onClick={onOpenDownload}>
                去官网安装
            </Button>
        );

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
                <div className="h-full min-w-0">
                    <ComponentManageCard
                        accent={isInstalling ? 'brand' : 'none'}
                        statusBadge={dockerRowStatusBadge({
                            ready,
                            probing,
                            inFlight: isInstalling,
                        })}
                        title="Docker"
                        icon={<Container size={18} strokeWidth={2} className="text-info" />}
                        description="Linux 容器引擎，支持容器化隔离运行 NapCat 与 SnowLuma"
                        titleAside={
                            <button
                                type="button"
                                onClick={() => openExternal('https://www.docker.com/')}
                                title="访问 Docker 官网"
                                aria-label="访问 Docker 官网"
                                className="inline-flex h-6 w-6 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-brand"
                            >
                                <ExternalLink size={13} strokeWidth={2} aria-hidden />
                            </button>
                        }
                        meta={
                            <DockerMeta
                                ready={ready}
                                summary={summary}
                                probing={probing}
                                isInstalling={isInstalling}
                                installHint={installHint}
                                installProgress={installProgress}
                            />
                        }
                        footer={footer}
                        progressOverlay={
                            isInstalling &&
                            installProgress &&
                            shouldShowProgressBar(installProgress) ? (
                                <ProgressBarOverlay progress={installProgress} determinate />
                            ) : undefined
                        }
                    />
                </div>
            </ContextMenuTrigger>

            <ContextMenuContent className="w-52">
                <ContextMenuLabel className="font-mono text-2xs truncate">
                    Docker 运行环境
                </ContextMenuLabel>
                <ContextMenuSeparator />

                {!ready && autoInstallable && (
                    <ContextMenuItem tone="brand" disabled={isInstalling} onClick={onInstall}>
                        <Download size={13} className="text-brand" />
                        <span>自动安装 Docker</span>
                    </ContextMenuItem>
                )}

                {!ready && !autoInstallable && (
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

const DockerMeta: React.FC<{
    ready: boolean;
    summary: { ready: boolean; label: string } | null;
    probing: boolean;
    isInstalling?: boolean;
    installHint?: string;
    installProgress?: ActionProgressView | null;
}> = ({ ready, summary, probing, isInstalling, installHint, installProgress }) => {
    if (isInstalling && installProgress) {
        const step =
            installProgress.totalSteps > 0 && installProgress.currentStep > 0
                ? `${installProgress.currentStep}/${installProgress.totalSteps} · `
                : '';
        return (
            <p className="truncate text-xs text-text-secondary">
                {step}
                {installProgress.message || installHint || '正在安装…'}
            </p>
        );
    }
    if (isInstalling) {
        return <p className="truncate text-xs text-text-secondary">{installHint ?? '正在安装…'}</p>;
    }
    if (probing) {
        return <p className="truncate text-xs text-text-tertiary">正在探测 Docker…</p>;
    }
    if (ready) {
        return (
            <p className="truncate font-mono text-xs tabular-nums text-text-tertiary">
                {summary?.label ?? 'Docker 就绪'}
            </p>
        );
    }
    return (
        <p className="truncate text-xs text-text-tertiary">{summary?.label ?? '未检测到 Docker'}</p>
    );
};

export default DockerRow;
