// 运行时依赖里的 Docker 行。
//
// Docker 不在后端组件 catalog 里（它是独立的 DockerCli，不走组件 action 管线），
// 但从用户心智看它就是一项运行时依赖，和 Node.js / QQ / noVNC 并列。所以这里
// 单独做一行，视觉对齐 MachineComponentRowView，但状态来自 docker 探测、动作走
// docker hook：
//   - 已就绪 → 显示版本，绿点
//   - Linux 远端未装 → 「安装」按钮（走 get.docker.com 脚本，真装）
//   - Windows / macOS 本机未装 → 「去官网装」按钮（开 Docker Desktop 下载页）
//   - 探测中 → 占位

import React from 'react';
import { Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DockerStatus, Os } from '../../core/ipc/types';
import { ProgressLine, shouldShowProgressBar, ProgressBarOverlay } from './progressView';
import { ComponentCardBody, ComponentEntityCard } from './ComponentEntityCard';

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
    // Linux 才能用脚本自动装；Windows / macOS 需要手动装 Docker Desktop。
    const autoInstallable = os === 'linux';

    const footer =
        ready || (isProbing && !status)
            ? undefined
            : autoInstallable ? (
                  <Button size="sm" variant="primary" onClick={onInstall} disabled={isInstalling}>
                      {isInstalling && (
                          <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
                      )}
                      安装
                  </Button>
              ) : (
                  <Button size="sm" variant="secondary" onClick={onOpenDownload}>
                      去官网装
                  </Button>
              );

    return (
        <ComponentEntityCard footer={footer}>
            <ComponentCardBody
                statusDot={<StatusDot ready={ready} probing={isProbing && !status} />}
                titleRow={
                    <div className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 flex-1 truncate font-display text-md font-semibold leading-tight text-text">
                            Docker
                        </span>
                        <DockerChip ready={ready} probing={isProbing && !status} />
                        <a
                            href="https://www.docker.com/"
                            onClick={(e) => {
                                e.preventDefault();
                                openExternal('https://www.docker.com/');
                            }}
                            className="ml-auto shrink-0 cursor-pointer text-2xs text-text-tertiary hover:text-text"
                        >
                            官网
                        </a>
                    </div>
                }
                description="容器运行时，用于以容器方式部署 NapCat / SnowLuma"
                statusLine={
                    <StatusLine
                        ready={ready}
                        summary={summary}
                        isProbing={isProbing && !status}
                        isInstalling={isInstalling}
                        installHint={installHint}
                        installProgress={installProgress}
                    />
                }
            />
        </ComponentEntityCard>
    );
};

const StatusDot: React.FC<{ ready: boolean; probing: boolean }> = ({ ready, probing }) => {
    const cls = probing
        ? 'bg-warning'
        : ready
            ? 'bg-success shadow-glow-success'
            : 'bg-text-disabled';
    return <span aria-hidden className={`mt-1.5 inline-block h-2.5 w-2.5 shrink-0 rounded-full ${cls}`} />;
};

const DockerChip: React.FC<{ ready: boolean; probing: boolean }> = ({ ready, probing }) => {
    const chip = probing
        ? { label: '探测中', cls: 'bg-inset text-text-tertiary' }
        : ready
            ? { label: '就绪', cls: 'bg-success-soft text-success' }
            : { label: '未就绪', cls: 'bg-inset text-text-tertiary' };
    return (
        <span className={`shrink-0 rounded-xs px-1.5 py-0.5 text-2xs font-medium ${chip.cls}`}>
            {chip.label}
        </span>
    );
};

const StatusLine: React.FC<{
    ready: boolean;
    summary: { ready: boolean; label: string } | null;
    isProbing: boolean;
    isInstalling?: boolean;
    installHint?: string;
    installProgress?: ActionProgressView | null;
}> = ({ ready, summary, isProbing, isInstalling, installHint, installProgress }) => {
    if (isInstalling && installProgress) {
        return (
            <div className="mt-1 min-w-0">
                <ProgressLine progress={installProgress} />
                {shouldShowProgressBar(installProgress) && (
                    <ProgressBarOverlay progress={installProgress} />
                )}
            </div>
        );
    }
    if (isInstalling) {
        return (
            <p className="mt-1 truncate text-xs text-warning">
                {installHint ?? '正在安装…'}
            </p>
        );
    }
    if (isProbing) {
        return <p className="mt-1 truncate text-xs text-text-tertiary">正在探测</p>;
    }
    if (ready) {
        return (
            <p className="mt-1 truncate font-mono text-xs tabular-nums text-text-tertiary">
                {summary?.label ?? 'Docker 就绪'}
            </p>
        );
    }
    // 未就绪：可能是没装、daemon 没起、缺 compose。summary.label 已经写清楚。
    return (
        <p className="mt-1 line-clamp-2 text-xs text-text-tertiary">
            {summary?.label ?? '未安装'}
        </p>
    );
};

export default DockerRow;
