// 单个容器卡片：名字 + 镜像 + 状态徽章 + 端口 + 操作按钮（起/停/重启/删/日志）。

import React from 'react';
import { Play, Square, RotateCw, Trash2, ScrollText } from 'lucide-react';
import { ActionMotionIcon } from '../../shared/ui/motion';
import { Button } from '../../shared/ui';
import { containerStateBadge, isManagedImage, compactPorts } from '../../core/domain/docker/status';
import type { ContainerAction, ContainerInfo } from '../../core/ipc/types';

interface ContainerCardProps {
    container: ContainerInfo;
    isActing: boolean;
    onAction: (action: ContainerAction) => void;
    onViewLogs: () => void;
}

const TONE_CLASS: Record<string, string> = {
    success: 'bg-success-soft text-success',
    danger: 'bg-danger-soft text-danger',
    warning: 'bg-warning-soft text-warning',
    neutral: 'bg-inset text-text-secondary',
};

export const ContainerCard: React.FC<ContainerCardProps> = ({
    container,
    isActing,
    onAction,
    onViewLogs,
}) => {
    const badge = containerStateBadge(container.state);
    const running = container.state === 'running';
    const managed = isManagedImage(container.image);
    // 去重压缩端口（IPv4/IPv6 合一）；单行展示，超出折成 +N。固定一行高度让
    // 每张卡固有高度一致，grid 不再因某张卡端口多而把整行拉高。
    const ports = compactPorts(container.ports);
    const PORTS_SHOWN = 2;
    const shownPorts = ports.slice(0, PORTS_SHOWN);
    const extraPorts = ports.length - shownPorts.length;

    return (
        <div className="flex h-full flex-col gap-3 rounded-md border border-border-subtle bg-surface p-4">
            <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                    <div className="flex items-center gap-2">
                        <span className="truncate font-display text-sm font-semibold text-text">
                            {container.name}
                        </span>
                        <span
                            className={`shrink-0 rounded-xs px-1.5 py-0.5 text-2xs font-medium ${TONE_CLASS[badge.tone]}`}
                        >
                            {badge.label}
                        </span>
                        {managed && (
                            <span className="shrink-0 rounded-xs bg-brand-soft px-1.5 py-0.5 text-2xs font-medium text-brand">
                                托管
                            </span>
                        )}
                    </div>
                    <p className="mt-0.5 truncate text-xs text-text-tertiary">
                        {container.image}
                    </p>
                </div>
            </div>

            <p className="truncate text-xs text-text-secondary">{container.status}</p>

            {/* 固定一行端口槽：始终占位（无端口时显示占位短横），保证所有卡固有
                高度一致。端口多时取前 2 条 + 悬停 title 看全量，再加 +N 计数。 */}
            <div className="flex min-h-[1.5rem] items-center gap-1 overflow-hidden">
                {shownPorts.length === 0 ? (
                    <span className="text-2xs text-text-disabled">无端口映射</span>
                ) : (
                    <>
                        {shownPorts.map((p) => (
                            <span
                                key={p}
                                title={p}
                                className="shrink-0 rounded-xs bg-inset/60 px-1.5 py-0.5 font-mono text-2xs text-text-tertiary"
                            >
                                {p}
                            </span>
                        ))}
                        {extraPorts > 0 && (
                            <span
                                title={ports.join('\n')}
                                className="shrink-0 rounded-xs bg-inset/60 px-1.5 py-0.5 font-mono text-2xs text-text-tertiary"
                            >
                                +{extraPorts}
                            </span>
                        )}
                    </>
                )}
            </div>

            <div className="mt-auto flex flex-wrap gap-1.5 border-t border-border-subtle pt-3">
                {running ? (
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => onAction('stop')}
                        disabled={isActing}
                    >
                        <ActionMotionIcon icon={Square} size={13} />
                        停止
                    </Button>
                ) : (
                    <Button
                        size="sm"
                        onClick={() => onAction('start')}
                        disabled={isActing}
                    >
                        <ActionMotionIcon icon={Play} size={13} />
                        启动
                    </Button>
                )}
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => onAction('restart')}
                    disabled={isActing}
                >
                    <ActionMotionIcon icon={RotateCw} size={13} />
                    重启
                </Button>
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={onViewLogs}
                    title="查看容器运行日志（docker logs）"
                    aria-label={`查看容器 ${container.name} 的运行日志`}
                >
                    <ActionMotionIcon icon={ScrollText} size={13} />
                    日志
                </Button>
                <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => onAction('remove')}
                    disabled={isActing}
                    className="text-danger hover:bg-danger-soft"
                >
                    <ActionMotionIcon icon={Trash2} size={13} />
                    删除
                </Button>
            </div>
        </div>
    );
};
