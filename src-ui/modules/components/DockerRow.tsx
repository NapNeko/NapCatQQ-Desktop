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
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { DockerStatus, Os } from '../../core/ipc/types';

interface DockerRowProps {
    os: Os;
    status: DockerStatus | undefined;
    isProbing: boolean;
    isInstalling: boolean;
    onInstall: () => void;
    onOpenDownload: () => void;
}

export const DockerRow: React.FC<DockerRowProps> = ({
    os,
    status,
    isProbing,
    isInstalling,
    onInstall,
    onOpenDownload,
}) => {
    const summary = status ? dockerStatusSummary(status) : null;
    const ready = summary?.ready ?? false;
    // Linux 才能用脚本自动装；Windows / macOS 需要手动装 Docker Desktop。
    const autoInstallable = os === 'linux';

    return (
        <div className="group relative flex h-full flex-col gap-2 overflow-hidden rounded-md bg-elevated px-4 py-3 shadow-card ring-1 ring-border-subtle transition-all duration-150 hover:bg-elevated/90 hover:shadow-popover">
            <div className="flex items-start gap-2.5">
                <StatusDot ready={ready} probing={isProbing && !status} />
                <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                        <span className="truncate font-display text-md font-semibold leading-tight text-text">
                            Docker
                        </span>
                        <DockerChip ready={ready} probing={isProbing && !status} />
                        <a
                            href="https://www.docker.com/"
                            target="_blank"
                            rel="noopener noreferrer"
                            className="ml-auto shrink-0 text-2xs text-text-tertiary hover:text-text"
                        >
                            官网
                        </a>
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs leading-snug text-text-secondary">
                        容器运行时，用于以容器方式部署 NapCat / SnowLuma
                    </p>
                    <StatusLine ready={ready} summary={summary} isProbing={isProbing && !status} />
                </div>
            </div>
            <div className="mt-auto flex items-center justify-end gap-1.5">
                {ready || (isProbing && !status) ? null : autoInstallable ? (
                    <Button size="sm" variant="primary" onClick={onInstall} disabled={isInstalling}>
                        {isInstalling && <Loader2 size={13} className="animate-spin" />}
                        安装
                    </Button>
                ) : (
                    <Button size="sm" variant="secondary" onClick={onOpenDownload}>
                        去官网装
                    </Button>
                )}
            </div>
        </div>
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
}> = ({ ready, summary, isProbing }) => {
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
