// 运行时依赖里的 Docker 行（视觉对齐 ComponentManageCard）。

import React from 'react';
import { ExternalLink, Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DockerStatus, Os } from '../../core/ipc/types';
import { ProgressLine, shouldShowProgressBar, ProgressBarOverlay } from './progressView';
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
        ready || probing ? (
            <span className="text-2xs text-text-disabled">—</span>
        ) : autoInstallable ? (
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

    return (
        <ComponentManageCard
            accent={isInstalling ? 'brand' : 'none'}
            statusBadge={dockerRowStatusBadge({ ready, probing, inFlight: isInstalling })}
            title="Docker"
            titleAside={
                <button
                    type="button"
                    onClick={() => openExternal('https://www.docker.com/')}
                    className="inline-flex items-center gap-0.5 text-2xs text-text-tertiary transition-colors hover:text-brand"
                >
                    官网
                    <ExternalLink size={11} strokeWidth={2} aria-hidden />
                </button>
            }
            description="容器运行时，用于以容器方式部署 NapCat / SnowLuma"
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
                isInstalling && installProgress && shouldShowProgressBar(installProgress) ? (
                    <ProgressBarOverlay progress={installProgress} />
                ) : undefined
            }
        />
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
        return <ProgressLine progress={installProgress} className="mt-0" />;
    }
    if (isInstalling) {
        return (
            <p className="truncate text-xs text-text-secondary">
                {installHint ?? '正在安装…'}
            </p>
        );
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
        <p className="truncate text-xs text-text-tertiary">
            {summary?.label ?? '未检测到 Docker'}
        </p>
    );
};

export default DockerRow;