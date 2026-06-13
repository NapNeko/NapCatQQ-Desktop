// 单个容器卡片：名字 + 镜像 + 状态徽章 + 端口 + 操作按钮（起/停/重启/删/日志）。
// 视觉对齐 ComponentManageCard / ServerCard：surface 壳 + 底栏（状态 + 操作）。

import React from 'react';
import { Box, Play, Square, RotateCw, Trash2, ScrollText } from 'lucide-react';
import { ActionMotionIcon, RESOURCE_MOTION } from '../../shared/ui/motion';
import { Badge, Button } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { containerStateBadge, isManagedImage, compactPorts } from '../../core/domain/docker/status';
import type { ContainerAction, ContainerInfo } from '../../core/ipc/types';

interface ContainerCardProps {
    container: ContainerInfo;
    isActing: boolean;
    onAction: (action: ContainerAction) => void;
    onViewLogs: () => void;
}

export const ContainerCard: React.FC<ContainerCardProps> = ({
    container,
    isActing,
    onAction,
    onViewLogs,
}) => {
    const badge = containerStateBadge(container.state);
    const running = container.state === 'running';
    const managed = isManagedImage(container.image);
    const ports = compactPorts(container.ports);
    const PORTS_SHOWN = 2;
    const shownPorts = ports.slice(0, PORTS_SHOWN);
    const extraPorts = ports.length - shownPorts.length;
    const statusLine =
        container.status.trim() !== '' ? container.status : '暂无 Docker 状态文案';

    return (
        <article
            className={cn(
                'relative isolate flex h-full w-full min-w-0 flex-col overflow-hidden ' +
                'rounded-md border border-border-subtle bg-surface shadow-card ' +
                'transition-[box-shadow] duration-200 hover:shadow-popover',
            )}
        >
            <div className="flex min-h-0 flex-1 flex-col gap-2 px-3.5 pb-2 pt-3">
                <div className="flex min-w-0 items-start gap-2.5">
                    <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-brand-soft text-brand">
                        <ActionMotionIcon icon={Box} size={18} motion={RESOURCE_MOTION} />
                    </div>
                    <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center gap-2">
                            <h3 className="min-w-0 flex-1 truncate font-display text-base font-semibold leading-snug text-text">
                                {container.name}
                            </h3>
                            {managed ? (
                                <Badge tone="brand" appearance="soft" className="shrink-0">
                                    托管
                                </Badge>
                            ) : null}
                        </div>
                        <p className="mt-0.5 truncate font-mono text-xs text-text-tertiary">
                            {container.image}
                        </p>
                    </div>
                </div>

                <p
                    className="line-clamp-2 min-h-[2.25rem] text-xs leading-snug text-text-secondary"
                    title={statusLine}
                >
                    {statusLine}
                </p>

                <div className="flex min-h-[1.5rem] min-w-0 items-center gap-1 overflow-hidden">
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
            </div>

            <footer className="flex min-h-[2.75rem] shrink-0 flex-wrap items-center justify-between gap-x-2 gap-y-1.5 border-t border-border-subtle bg-inset/40 px-3 py-2">
                <Badge
                    tone={badge.tone}
                    appearance="soft"
                    dot={running}
                    className="max-w-[40%] shrink-0 truncate"
                >
                    {badge.label}
                </Badge>
                <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-1.5">
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
                        <Button size="sm" onClick={() => onAction('start')} disabled={isActing}>
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
            </footer>
        </article>
    );
};