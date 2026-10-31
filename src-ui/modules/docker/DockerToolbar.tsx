// Docker 页顶栏保留当前主机上下文，部署与资源管理共用同一个入口。

import type { ReactNode } from 'react';
import * as RadixSelect from '@radix-ui/react-select';
import {
    CheckCircle2,
    AlertTriangle,
    ChevronDown,
    Container,
    RefreshCw,
    Server,
} from 'lucide-react';
import { Button, Checkbox } from '../../shared/ui';
import { ActionMotionIcon, RESOURCE_MOTION, refreshMotion } from '../../shared/ui/motion';
import { cn } from '../../shared/utils/cn';
import { localOs } from '../../core/domain/runtime/env';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

interface DockerToolbarProps {
    hostId: string | null;
    servers: ServerProfile[];
    onChangeHost: (hostId: string) => void;
    summary: { ready: boolean; label: string } | null;
    isProbing: boolean;
    resourceCount: number | null;
    resourceLabel: '容器' | '镜像' | '网络' | '数据卷';
    autoRefresh: boolean;
    onAutoRefreshChange: (enabled: boolean) => void;
    isRefreshing: boolean;
    onRefresh: () => void;
    onOpenDeployment: () => void;
    children: ReactNode;
}

export function DockerToolbar({
    hostId,
    servers,
    onChangeHost,
    summary,
    isProbing,
    resourceCount,
    resourceLabel,
    autoRefresh,
    onAutoRefreshChange,
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
        <header className="shrink-0 space-y-4 pb-4 pt-1">
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3">
                <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
                    <h1 className="shrink-0 font-display text-xl font-semibold text-text">
                        容器管理
                    </h1>
                    <span aria-hidden className="hidden h-5 w-px bg-border-subtle sm:block" />
                    <HostSelect items={items} value={hostId ?? undefined} onChange={onChangeHost} />
                    <StatusPill
                        ready={ready}
                        label={summary?.label ?? null}
                        isProbing={isProbing}
                    />
                </div>
                <div className="flex shrink-0 flex-wrap items-center gap-2.5">
                    <Checkbox
                        id="docker-auto"
                        label="自动刷新"
                        checked={autoRefresh}
                        onCheckedChange={onAutoRefreshChange}
                    />
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
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle pb-3">
                {children}
                {ready && resourceCount != null && (
                    <span className="text-xs text-text-tertiary tabular-nums">
                        共 {resourceCount} 个{resourceLabel}
                    </span>
                )}
            </div>
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
                    'inline-flex min-w-0 max-w-full items-center gap-2 rounded-sm border border-border-subtle bg-field px-2.5 py-1.5 sm:max-w-[26rem]',
                    'text-[13px] text-text outline-none transition-colors',
                    'hover:bg-inset/40 focus:border-brand focus:ring-1 focus:ring-brand',
                )}
                aria-label="Docker 主机"
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
    ready,
    label,
    isProbing,
}: {
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
                探测 Docker…
            </span>
        );
    }
    if (ready) {
        return (
            <span className="inline-flex items-center gap-1.5 text-xs text-success">
                <ActionMotionIcon icon={CheckCircle2} size={13} />
                {label ?? 'Docker 就绪'}
            </span>
        );
    }
    return (
        <span className="inline-flex items-center gap-1.5 text-xs text-warning">
            <ActionMotionIcon icon={AlertTriangle} size={13} />
            {label ?? '无法探测 Docker 状态'}
        </span>
    );
}

export default DockerToolbar;
