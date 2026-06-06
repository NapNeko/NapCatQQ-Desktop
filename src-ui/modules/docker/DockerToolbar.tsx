// Docker 页工具条：把"选主机 + Docker 状态 + 容器数"收成一行。
//
// 原来这三件事是三层全宽大块（带边框的大选择器 + 整条状态横幅 + 容器数标题），
// 进容器列表前要竖着穿过去，挤占主内容。改成一行紧凑工具条：左边主机选择器，
// 紧跟一个状态小药丸（就绪时退成周边信息，符合 calm technology），右边容器数。
// 容器网格因此能直接顶上来当主内容。
//
// Docker 没就绪时药丸变 warning 色，并在工具条下方补一行引导（需要行动才升级
// 视觉强度）。

import React from 'react';
import * as RadixSelect from '@radix-ui/react-select';
import { CheckCircle2, AlertTriangle, ChevronDown, RefreshCw, Server } from 'lucide-react';
import {
    ActionMotionIcon,
    RESOURCE_MOTION,
    refreshMotion,
} from '../../shared/ui/motion';
import { cn } from '../../shared/utils/cn';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

interface DockerToolbarProps {
    hostId: string | null;
    servers: ServerProfile[];
    onChangeHost: (hostId: string) => void;
    summary: { ready: boolean; label: string } | null;
    isProbing: boolean;
    containerCount: number | null;
}

export const DockerToolbar: React.FC<DockerToolbarProps> = ({
    hostId,
    servers,
    onChangeHost,
    summary,
    isProbing,
    containerCount,
}) => {
    const items = servers.map((s) => ({
        value: `remote:${s.id}`,
        label: `${s.name}（${s.host}）`,
    }));
    const ready = summary?.ready ?? false;

    return (
        <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center gap-3">
                <HostSelect items={items} value={hostId ?? undefined} onChange={onChangeHost} />
                <StatusPill ready={ready} label={summary?.label ?? null} isProbing={isProbing} />
                {ready && containerCount != null && (
                    <span className="ml-auto text-xs text-text-tertiary">
                        共 {containerCount} 个容器
                    </span>
                )}
            </div>
            {!ready && !isProbing && (
                <p className="text-xs text-text-tertiary">
                    去组件页的「Docker 部署」区安装 / 启动 Docker。
                </p>
            )}
        </div>
    );
};

// 紧凑内联主机选择器：左侧固定一个 server 图标 + 当前值，点开下拉切主机。
// 不用 shared Select 的「label 在上、整块大边框」形态，工具条里要的是行内紧凑。
const HostSelect: React.FC<{
    items: { value: string; label: string }[];
    value: string | undefined;
    onChange: (v: string) => void;
}> = ({ items, value, onChange }) => (
    <RadixSelect.Root value={value} onValueChange={onChange}>
        <RadixSelect.Trigger
            className={cn(
                'inline-flex items-center gap-2 rounded-md border border-border-subtle bg-surface px-3 py-2',
                'text-sm text-text outline-none transition-colors',
                'hover:bg-inset/40 focus:border-brand focus:ring-1 focus:ring-brand',
            )}
        >
            <ActionMotionIcon
                icon={Server}
                size={14}
                motion={RESOURCE_MOTION}
                className="text-text-tertiary"
            />
            <RadixSelect.Value />
            <RadixSelect.Icon asChild>
                <ActionMotionIcon
                    icon={ChevronDown}
                    size={14}
                    className="text-text-tertiary"
                />
            </RadixSelect.Icon>
        </RadixSelect.Trigger>
        <RadixSelect.Portal>
            <RadixSelect.Content
                position="popper"
                sideOffset={4}
                className={cn(
                    'z-50 overflow-hidden rounded-sm border border-border-subtle bg-elevated shadow-popover',
                    'min-w-[var(--radix-select-trigger-width)]',
                )}
            >
                <RadixSelect.Viewport className="p-1">
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

const StatusPill: React.FC<{
    ready: boolean;
    label: string | null;
    isProbing: boolean;
}> = ({ ready, label, isProbing }) => {
    if (isProbing && !label) {
        return (
            <span className="inline-flex items-center gap-1.5 rounded-md bg-inset/60 px-2.5 py-1.5 text-xs text-text-tertiary">
                <ActionMotionIcon
                    icon={RefreshCw}
                    size={13}
                    motion={refreshMotion(isProbing)}
                />
                探测 Docker…
            </span>
        );
    }
    if (ready) {
        return (
            <span className="inline-flex items-center gap-1.5 rounded-md bg-success-soft px-2.5 py-1.5 text-xs text-success">
                <ActionMotionIcon icon={CheckCircle2} size={13} />
                {label ?? 'Docker 就绪'}
            </span>
        );
    }
    return (
        <span className="inline-flex items-center gap-1.5 rounded-md bg-warning-soft px-2.5 py-1.5 text-xs text-warning">
            <ActionMotionIcon icon={AlertTriangle} size={13} />
            {label ?? '无法探测 Docker 状态'}
        </span>
    );
};

export default DockerToolbar;
