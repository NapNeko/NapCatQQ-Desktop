// Docker 页顶栏保留当前主机上下文，部署与资源管理共用同一个入口。

import type { ReactNode } from 'react';
import * as RadixSelect from '@radix-ui/react-select';
import { AlertTriangle, ChevronDown, Container, RefreshCw, Server } from 'lucide-react';
import { Button } from '../../shared/ui';
import { ActionMotionIcon, RESOURCE_MOTION, refreshMotion } from '../../shared/ui/motion';
import { cn } from '../../shared/utils/cn';
import { localOs } from '../../core/domain/runtime/env';
import { containerEngineLabel } from '../../core/domain/docker/engine';
import { ContainerEngineSelect } from '../../shared/components/ContainerEngineSelect';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

interface DockerToolbarProps {
    hostId: string | null;
    servers: ServerProfile[];
    onChangeHost: (hostId: string) => void;
    engine?: ContainerEngine;
    onChangeEngine: (engine: ContainerEngine) => void;
    engineDisabled: boolean;
    summary: { ready: boolean; label: string } | null;
    isProbing: boolean;
    resourceCount: number | null;
    resourceLabel: '容器' | '镜像' | '网络' | '数据卷';
    isRefreshing: boolean;
    onRefresh: () => void;
    onOpenDeployment: () => void;
    children: ReactNode;
}

export function DockerToolbar({
    hostId,
    servers,
    onChangeHost,
    engine,
    onChangeEngine,
    engineDisabled,
    summary,
    isProbing,
    resourceCount,
    resourceLabel,
    isRefreshing,
    onRefresh,
    onOpenDeployment,
    children,
}: DockerToolbarProps) {
    const items = servers.map((s) => ({
        value: `remote:${s.id}`,
        label: `${s.name}（${s.host}）`,
    }));
    if (localOs() === 'linux') items.unshift({ value: 'local', label: '本机 Linux' });
    const ready = summary?.ready ?? false;

    return (
        <header className="shrink-0 space-y-3 pb-4 pt-1">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">
                    <h1 className="shrink-0 font-display text-xl font-semibold text-text">
                        容器管理
                    </h1>
                    {ready && resourceCount != null && (
                        <span className="text-xs text-text-tertiary tabular-nums">
                            共 {resourceCount} 个{resourceLabel}
                        </span>
                    )}
                </div>
                <div className="flex shrink-0 items-center gap-2">
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={onRefresh}
                        disabled={isRefreshing}
                    >
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            motion={refreshMotion(isRefreshing)}
                        />
                        刷新
                    </Button>
                    <Button size="sm" variant="primary" onClick={onOpenDeployment}>
                        <Container size={14} />
                        容器部署
                    </Button>
                </div>
            </div>
            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
                <HostSelect items={items} value={hostId ?? undefined} onChange={onChangeHost} />
                <ContainerEngineSelect
                    size="sm"
                    value={engine}
                    onValueChange={onChangeEngine}
                    disabled={engineDisabled}
                />
                <StatusPill
                    engineName={containerEngineLabel(engine)}
                    ready={ready}
                    label={summary?.label ?? null}
                    isProbing={isProbing}
                />
            </div>
            <div className="border-b border-border-subtle pb-3">{children}</div>
        </header>
    );
}

function HostSelect({
    items,
    value,
    onChange,
}: {
    items: { value: string; label: string }[];
    value: string | undefined;
    onChange: (v: string) => void;
}) {
    return (
        <RadixSelect.Root value={value} onValueChange={onChange}>
            <RadixSelect.Trigger
                className={cn(
                    'inline-flex h-7 min-w-0 max-w-full items-center gap-2 rounded-sm border border-border-subtle bg-field px-2.5 sm:max-w-[22rem]',
                    'text-xs text-text outline-none transition-colors',
                    'hover:bg-inset/40 focus:border-brand focus:ring-1 focus:ring-brand',
                )}
                aria-label="容器主机"
            >
                <ActionMotionIcon
                    icon={Server}
                    size={14}
                    motion={RESOURCE_MOTION}
                    className="text-text-tertiary"
                />
                <span className="min-w-0 flex-1 truncate text-left">
                    <RadixSelect.Value />
                </span>
                <RadixSelect.Icon asChild>
                    <ActionMotionIcon icon={ChevronDown} size={14} className="text-text-tertiary" />
                </RadixSelect.Icon>
            </RadixSelect.Trigger>
            <RadixSelect.Portal>
                <RadixSelect.Content
                    position="popper"
                    sideOffset={4}
                    className={cn(
                        'z-50 overflow-hidden rounded-sm border border-border-subtle bg-elevated shadow-popover',
                        'min-w-[var(--radix-select-trigger-width)] max-w-[calc(100vw-2rem)]',
                    )}
                >
                    <RadixSelect.Viewport className="max-h-[var(--radix-select-content-available-height)] overflow-y-auto p-1">
                        {items.map((item) => (
                            <RadixSelect.Item
                                key={item.value}
                                value={item.value}
                                className={cn(
                                    'relative flex cursor-pointer select-none items-center rounded-xs px-2.5 py-1.5 text-sm text-text',
                                    'data-[state=checked]:bg-brand-soft data-[state=checked]:text-brand',
                                    'data-[highlighted]:bg-inset data-[highlighted]:outline-none',
                                )}
                            >
                                <RadixSelect.ItemText>{item.label}</RadixSelect.ItemText>
                            </RadixSelect.Item>
                        ))}
                    </RadixSelect.Viewport>
                </RadixSelect.Content>
            </RadixSelect.Portal>
        </RadixSelect.Root>
    );
}

function StatusPill({
    engineName,
    ready,
    label,
    isProbing,
}: {
    engineName: string;
    ready: boolean;
    label: string | null;
    isProbing: boolean;
}) {
    if (isProbing && !label) {
        return (
            <span
                role="status"
                className="inline-flex items-center gap-1.5 text-xs text-text-tertiary"
            >
                <ActionMotionIcon icon={RefreshCw} size={13} motion={refreshMotion(isProbing)} />
                正在读取 {engineName} 状态…
            </span>
        );
    }
    if (ready) {
        return (
            <span className="inline-flex items-center gap-1.5 text-xs text-success">
                <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-current" />
                {label ?? `${engineName} 就绪`}
            </span>
        );
    }
    return (
        <span className="inline-flex items-center gap-1.5 text-xs text-warning">
            <ActionMotionIcon icon={AlertTriangle} size={13} />
            {label ?? '等待引擎状态'}
        </span>
    );
}

export default DockerToolbar;
