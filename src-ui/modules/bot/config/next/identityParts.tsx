import type { ComponentType, ReactNode } from 'react';
import {
    AlertTriangle,
    CheckCircle2,
    CircleHelp,
    Flame,
    Info,
    Snowflake,
    type LucideProps,
} from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '../../../../shared/ui';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import { dockerReadinessNotice } from '../../../../core/domain/bot/docker-start-gate';
import { cn } from '../../../../shared/utils/cn';
import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { SnowLumaStartMode } from '../../../../core/ipc/generated/domain/SnowLumaStartMode';

export function SelectionDot({ selected }: { selected: boolean }) {
    return (
        <span
            aria-hidden
            className={cn(
                'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors',
                selected ? 'border-brand bg-brand/10' : 'border-border-strong bg-field',
            )}
        >
            <span
                className={cn(
                    'h-2 w-2 rounded-full bg-brand transition-transform duration-150',
                    selected ? 'scale-100 opacity-100' : 'scale-50 opacity-0',
                )}
            />
        </span>
    );
}

export function InlineNotice({
    tone,
    children,
}: {
    tone: 'ok' | 'warn' | 'neutral' | 'danger';
    children: ReactNode;
}) {
    const cls =
        tone === 'ok'
            ? 'border-success/25 bg-success-soft/60 text-success'
            : tone === 'danger'
              ? 'border-danger/25 bg-danger-soft/60 text-danger'
              : tone === 'warn'
                ? 'border-warning/25 bg-warning-soft/60 text-warning'
                : 'border-border-subtle bg-inset text-text-secondary';
    const Icon =
        tone === 'ok' ? CheckCircle2 : tone === 'danger' || tone === 'warn' ? AlertTriangle : Info;
    return (
        <div
            className={cn(
                'flex items-start gap-2 rounded-sm border px-3 py-2 text-2xs leading-relaxed',
                cls,
            )}
        >
            <Icon size={13} className="mt-0.5 shrink-0" />
            <div className="min-w-0 flex-1">{children}</div>
        </div>
    );
}

export function SceneExplainerPopover({ children }: { children: ReactNode }) {
    return (
        <Popover>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="inline-flex h-6 items-center gap-1 rounded-sm px-1.5 text-[11px] text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                >
                    <CircleHelp size={12} />
                    <span>机制说明</span>
                </button>
            </PopoverTrigger>
            <PopoverContent side="bottom" align="end" sideOffset={6}>
                <p className="max-w-xs text-[12px] leading-relaxed text-text-secondary">
                    {children}
                </p>
            </PopoverContent>
        </Popover>
    );
}

export function DockerReadinessLine({
    engine,
    flavorLabel,
    status,
    probing,
    imageReady,
}: {
    flavorLabel: string;
    engine?: ContainerEngine;
    status: import('../../../../core/ipc/types').DockerStatus | undefined;
    probing: boolean;
    imageReady: boolean | undefined;
}) {
    const notice = dockerReadinessNotice({
        engine,
        flavorLabel,
        status,
        probing,
        imageReady,
    });
    if (!notice) return null;
    return <InlineNotice tone={notice.tone}>{notice.text}</InlineNotice>;
}

const SL_MODE_ITEMS: ReadonlyArray<{
    value: 'cold_start' | 'hot_start';
    label: string;
    summary: string;
    icon: ComponentType<LucideProps>;
}> = [
    {
        value: 'cold_start',
        label: '冷启动（默认）',
        summary: '由桌面端拉起并托管 QQ 进程',
        icon: Snowflake,
    },
    {
        value: 'hot_start',
        label: '热启动',
        summary: '附加到已登录此号的 QQ 进程',
        icon: Flame,
    },
];

export function SnowLumaStartModeBlock({
    data,
    onChange,
}: {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
}) {
    const mode: 'cold_start' | 'hot_start' =
        data.snowlumaStartMode?.mode === 'hot_start' ? 'hot_start' : 'cold_start';

    const setMode = (m: 'cold_start' | 'hot_start') => {
        const value: SnowLumaStartMode =
            m === 'cold_start' ? { mode: 'cold_start' } : { mode: 'hot_start' };
        onChange({ snowlumaStartMode: value });
    };

    return (
        <div className="flex flex-col">
            <div
                role="radiogroup"
                aria-label="SnowLuma 启动模式"
                className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            >
                {SL_MODE_ITEMS.map((item) => {
                    const selected = mode === item.value;
                    const Icon = item.icon;
                    return (
                        <button
                            key={item.value}
                            type="button"
                            role="radio"
                            aria-checked={selected}
                            onClick={() => setMode(item.value)}
                            className={cn(
                                'flex h-full min-w-0 items-center justify-between gap-3 rounded-md border px-3.5 py-2.5 text-left transition-all',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                selected
                                    ? 'border-brand/70 bg-brand-soft/20 shadow-xs ring-1 ring-brand/25'
                                    : 'border-border-subtle bg-field/45 hover:border-border hover:bg-field/80',
                            )}
                        >
                            <div className="flex min-w-0 items-center gap-2.5">
                                <span
                                    className={cn(
                                        'flex h-8 w-8 shrink-0 items-center justify-center rounded-sm border transition-colors',
                                        selected
                                            ? 'border-brand/30 bg-brand-soft text-brand'
                                            : 'border-border-subtle bg-inset text-text-secondary',
                                    )}
                                >
                                    <Icon size={15} strokeWidth={2.1} />
                                </span>
                                <div className="min-w-0">
                                    <div className="text-sm font-medium text-text">
                                        {item.label}
                                    </div>
                                    <div className="truncate text-2xs text-text-tertiary">
                                        {item.summary}
                                    </div>
                                </div>
                            </div>
                            <SelectionDot selected={selected} />
                        </button>
                    );
                })}
            </div>
            <ExpandPresence visible={mode === 'hot_start'}>
                <div className="pt-2.5">
                    <InlineNotice tone="neutral">
                        启动前需在此 QQ 号（
                        <span className="font-mono">{data.QQID || '未填'}</span>
                        ）下登录 QQ；找不到匹配进程时启动会失败
                    </InlineNotice>
                </div>
            </ExpandPresence>
        </div>
    );
}
