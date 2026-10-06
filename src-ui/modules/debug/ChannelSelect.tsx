// 顶栏的通道下拉，两个：「调用」（发请求走哪条）和「事件」（从哪条收事件）。
//
// 每行给出状态、地址、打码后的 token、能干什么，自带「测试连通」；做不了这件事的行禁用并写明原因。
// 状态不好的行照样能选——用户多半正要去修它，选上再测一次最顺手。

import {
    memo,
    useEffect,
    useRef,
    useState,
    type KeyboardEvent as ReactKeyboardEvent,
    type ReactNode,
} from 'react';
import { Bot, Check, ChevronDown, Package, RefreshCw, type LucideIcon } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Button, Popover, PopoverContent, PopoverTrigger, Spinner } from '../../shared/ui';
import { Shimmer, StatusDot, type StatusDotTone } from '../../shared/ui/motion';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useTestChannel } from '../../hooks/debug/useDebugChannels';
import { channelIdKey } from '../../hooks/debug/keys';
import {
    NO_CHANNEL_EXITS,
    channelShortLabel,
    channelStatusCopy,
} from '../../core/domain/debug/channelCopy';
import {
    AUTO_CHANNEL,
    channelSelectable,
    channelTriggerLabel,
    effectiveChannelId,
    findChannel,
    sameChannel,
    type ChannelPurpose,
} from '../../core/domain/debug/channelPick';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../core/ipc/generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../../core/ipc/generated/debug/DebugChannels';
import type { DebugChannelStatus } from '../../core/ipc/generated/debug/DebugChannelStatus';

const COPY: Record<
    ChannelPurpose,
    { prefix: string; title: string; hint: string; autoHint: string }
> = {
    call: {
        prefix: '调用',
        title: '调用通道',
        hint: '发请求走哪条。',
        autoHint: '按 内部通道 → WS → HTTP 的顺序挑第一条能用的',
    },
    events: {
        prefix: '事件',
        title: '事件来源',
        hint: '从哪条收事件（右栏的聊天和事件列表）。',
        autoHint: '按 内部通道 → WS 的顺序挑第一条能用的',
    },
};

const TONE_DOT: Record<ReturnType<typeof channelStatusCopy>['tone'], StatusDotTone> = {
    success: 'success',
    warning: 'warning',
    danger: 'danger',
    neutral: 'idle',
};

const TONE_TEXT: Record<ReturnType<typeof channelStatusCopy>['tone'], string> = {
    success: 'text-success',
    warning: 'text-warning',
    danger: 'text-danger',
    neutral: 'text-text-tertiary',
};

function statusTone(status: DebugChannelStatus | null | undefined): StatusDotTone {
    return status ? TONE_DOT[channelStatusCopy(status).tone] : 'idle';
}

export interface ChannelSelectProps {
    purpose: ChannelPurpose;
    botId: string | null;
    running: boolean;
    channels: DebugChannels | undefined;
    loading: boolean;
    error: boolean;
    onRetry: () => void;
    value: DebugChannelId;
    onChange: (id: DebugChannelId) => void;
    /** 受控打开：中栏的「查看通道」要能从外面把它点开 */
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    /** 页面跳转（「全都不可用」时给去组件页 / 机器人页的出口）；没给就不画 */
    onNavigate?: (route: AppRoute) => void;
}

export const ChannelSelect = memo(function ChannelSelect({
    purpose,
    botId,
    running,
    channels,
    loading,
    error,
    onRetry,
    value,
    onChange,
    open: openProp,
    onOpenChange,
    onNavigate,
}: ChannelSelectProps) {
    const m = useMotion();
    const [openLocal, setOpenLocal] = useState(false);
    const open = openProp ?? openLocal;
    const setOpen = (next: boolean) => {
        if (openProp === undefined) setOpenLocal(next);
        onOpenChange?.(next);
    };
    const listRef = useRef<HTMLDivElement>(null);
    const copy = COPY[purpose];

    const label = channelTriggerLabel(channels, value, purpose);
    const effective = findChannel(channels, effectiveChannelId(channels, value, purpose));
    const dotTone: StatusDotTone =
        loading && !channels
            ? 'idle'
            : label.none || label.missing
              ? 'danger'
              : statusTone(effective?.status);

    const choose = (id: DebugChannelId) => {
        onChange(id);
        setOpen(false);
    };

    // 点了出口就跳出调试台了，下拉顺手关上
    const navigateTo = (route: AppRoute) => {
        setOpen(false);
        onNavigate?.(route);
    };

    const onListKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        const list = Array.from(
            listRef.current?.querySelectorAll<HTMLButtonElement>(
                '[data-channel-option]:not(:disabled)',
            ) ?? [],
        );
        const idx = list.indexOf(document.activeElement as HTMLButtonElement);
        const next =
            e.key === 'ArrowDown' ? Math.min(list.length - 1, idx + 1) : Math.max(0, idx - 1);
        e.preventDefault();
        list[next]?.focus();
    };

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    disabled={!botId}
                    aria-label={`${copy.title}：${label.text}${label.missing ? '（已不存在）' : ''}`}
                    className={cn(
                        'group inline-flex h-8 min-w-0 max-w-[240px] shrink items-center gap-1.5 rounded-sm border border-border-subtle bg-surface pl-2 pr-1.5 text-left',
                        'transition-colors hover:border-border hover:bg-inset data-[state=open]:border-brand/50 data-[state=open]:bg-inset',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                    )}
                >
                    <span className="shrink-0 text-2xs font-medium text-text-tertiary">
                        {copy.prefix}
                    </span>
                    <StatusDot tone={dotTone} size={6} className="shrink-0" />
                    <span
                        className={cn(
                            'min-w-0 truncate text-[12.5px]',
                            label.none || label.missing ? 'text-danger' : 'text-text',
                        )}
                    >
                        {label.text}
                        {label.missing && '（已不存在）'}
                    </span>
                    <ChevronDown
                        size={13}
                        strokeWidth={2}
                        aria-hidden
                        className={cn(
                            'ml-auto shrink-0 text-text-tertiary group-data-[state=open]:rotate-180',
                            m.enabled && 'transition-transform duration-200',
                        )}
                    />
                </button>
            </PopoverTrigger>
            <PopoverContent
                align="start"
                className="flex w-[360px] flex-col overflow-hidden p-0"
                onOpenAutoFocus={(e) => {
                    e.preventDefault();
                    (
                        listRef.current?.querySelector<HTMLButtonElement>(
                            '[data-channel-option][aria-checked="true"]',
                        ) ??
                        listRef.current?.querySelector<HTMLButtonElement>('[data-channel-option]')
                    )?.focus();
                }}
            >
                <div className="border-b border-border-subtle/70 px-3 py-2.5">
                    <p className="font-display text-[13px] font-semibold text-text">{copy.title}</p>
                    <p className="mt-0.5 text-2xs text-text-tertiary">
                        {copy.hint}按 Bot 记住你的选择。
                    </p>
                </div>

                <div
                    ref={listRef}
                    role="radiogroup"
                    aria-label={copy.title}
                    onKeyDown={onListKeyDown}
                    className="max-h-[min(440px,62vh)] overflow-y-auto p-1"
                >
                    {!channels && loading ? (
                        <ChannelSkeleton />
                    ) : !channels && error ? (
                        <div className="flex flex-col items-center gap-2 px-3 py-6 text-center">
                            <p className="text-xs text-text-secondary">读不到这个 Bot 的通道列表</p>
                            <Button size="sm" variant="secondary" onClick={onRetry}>
                                <RefreshCw size={12} aria-hidden />
                                重试
                            </Button>
                        </div>
                    ) : channels ? (
                        <>
                            <AutoRow
                                purpose={purpose}
                                channels={channels}
                                selected={value.kind === 'auto'}
                                onChoose={() => choose(AUTO_CHANNEL)}
                                onNavigate={onNavigate ? navigateTo : undefined}
                            />
                            <div aria-hidden className="mx-2 my-1 h-px bg-border-subtle/70" />
                            {channels.channels.length === 0 ? (
                                <p className="px-3 py-3 text-2xs text-text-tertiary">
                                    这个 Bot 的配置里没有开 HTTP / WS 服务，只能走内部通道。
                                </p>
                            ) : (
                                channels.channels.map((c) => (
                                    <ChannelRow
                                        key={channelIdKey(c.id)}
                                        botId={channels.bot_id}
                                        info={c}
                                        purpose={purpose}
                                        running={running}
                                        selected={value.kind !== 'auto' && sameChannel(value, c.id)}
                                        onChoose={() => choose(c.id)}
                                    />
                                ))
                            )}
                        </>
                    ) : null}
                </div>

                {channels && !running && (
                    <p className="border-t border-border-subtle/70 bg-warning-soft/40 px-3 py-2 text-2xs text-text-secondary">
                        Bot 没在运行，通道状态要等它启动之后才准。
                    </p>
                )}
            </PopoverContent>
        </Popover>
    );
});

function RadioMark({ on }: { on: boolean }) {
    return (
        <span
            aria-hidden
            className={cn(
                'mt-0.5 inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border transition-colors',
                on ? 'border-brand bg-brand text-white' : 'border-border bg-surface',
            )}
        >
            {on && <Check size={9} strokeWidth={3.5} />}
        </span>
    );
}

function OptionButton({
    selected,
    disabled,
    onChoose,
    children,
    title,
}: {
    selected: boolean;
    disabled?: boolean;
    onChoose: () => void;
    children: ReactNode;
    title?: string;
}) {
    return (
        <button
            type="button"
            data-channel-option
            role="radio"
            aria-checked={selected}
            disabled={disabled}
            title={title}
            onClick={onChoose}
            className={cn(
                'flex min-w-0 flex-1 items-start gap-2.5 rounded-sm px-2 py-2 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand',
                'disabled:cursor-not-allowed',
                // hover 背景在最外层的行容器上：一行（选项 + 测试连通）是一张卡，不各亮各的
            )}
        >
            <RadioMark on={selected} />
            <span className="min-w-0 flex-1">{children}</span>
        </button>
    );
}

// 出口按钮的图标按目标页面来
const EXIT_ICON: Partial<Record<AppRoute, LucideIcon>> = { components: Package, bots: Bot };

function AutoRow({
    purpose,
    channels,
    selected,
    onChoose,
    onNavigate,
}: {
    purpose: ChannelPurpose;
    channels: DebugChannels;
    selected: boolean;
    onChoose: () => void;
    onNavigate?: (route: AppRoute) => void;
}) {
    const autoId = purpose === 'call' ? channels.auto_call : channels.auto_events;
    const autoInfo = findChannel(channels, autoId);
    const status = autoInfo ? channelStatusCopy(autoInfo.status) : null;
    return (
        <div
            className={cn(
                'rounded-sm transition-colors',
                selected ? 'bg-brand-soft/50 hover:bg-brand-soft/70' : 'hover:bg-inset',
            )}
        >
            <OptionButton selected={selected} onChoose={onChoose}>
                <span className="block text-[13px] font-medium text-text">自动</span>
                <span className="mt-0.5 block text-2xs text-text-tertiary">
                    {COPY[purpose].autoHint}
                </span>
                <span className="mt-1 flex min-w-0 items-center gap-1.5 text-2xs">
                    {autoId ? (
                        <>
                            <span className="text-text-secondary">
                                眼下：{channelShortLabel(autoId)}
                            </span>
                            {status && (
                                <span className={cn('truncate', TONE_TEXT[status.tone])}>
                                    · {status.text}
                                </span>
                            )}
                        </>
                    ) : (
                        <span className="text-danger">眼下没有能用的通道</span>
                    )}
                </span>
            </OptionButton>
            {/* 出口按钮放在选项按钮外面：不把它们的字并进「自动」的可访问名，点它也不会误选「自动」 */}
            {!autoId && onNavigate && (
                <div className="flex flex-wrap gap-1.5 px-2 pb-2 pl-8">
                    {NO_CHANNEL_EXITS.map((exit) => {
                        const ExitIcon = EXIT_ICON[exit.route];
                        return (
                            <Button
                                key={exit.route}
                                size="sm"
                                variant="secondary"
                                onClick={() => onNavigate(exit.route)}
                            >
                                {ExitIcon ? <ExitIcon size={12} aria-hidden /> : null}
                                {exit.label}
                            </Button>
                        );
                    })}
                </div>
            )}
        </div>
    );
}

type TestResult = { kind: 'ok'; status: DebugChannelStatus } | { kind: 'failed' };

/** 测完的结果在行里挂几秒就收起，状态本身已经写回列表了 */
const RESULT_LINGER_MS = 6000;

function ChannelRow({
    botId,
    info,
    purpose,
    running,
    selected,
    onChoose,
}: {
    botId: string;
    info: DebugChannelInfo;
    purpose: ChannelPurpose;
    running: boolean;
    selected: boolean;
    onChoose: () => void;
}) {
    // 每行一个 mutation：两行同时在测时各转各的圈
    const test = useTestChannel();
    const [result, setResult] = useState<TestResult | null>(null);
    const selectable = channelSelectable(info, purpose);
    const status = channelStatusCopy(info.status);

    useEffect(() => {
        if (!result) return;
        const t = setTimeout(() => setResult(null), RESULT_LINGER_MS);
        return () => clearTimeout(t);
    }, [result]);

    const runTest = () => {
        setResult(null);
        test.mutate(
            { botId, channel: info.id },
            {
                onSuccess: (next) => setResult({ kind: 'ok', status: next.status }),
                onError: () => setResult({ kind: 'failed' }),
            },
        );
    };

    const canTest = running && info.status.kind !== 'unsupported';

    return (
        // 整行一张卡：通道信息（左边整块可点）和「测试连通」在同一个 hover / 选中背景里，不画成两张卡
        <div
            className={cn(
                'rounded-sm transition-colors',
                selected ? 'bg-brand-soft/50 hover:bg-brand-soft/70' : 'hover:bg-inset',
                !selectable.ok && 'opacity-60',
            )}
        >
            <div className="flex items-start gap-1">
                <OptionButton
                    selected={selected}
                    disabled={!selectable.ok}
                    onChoose={onChoose}
                    title={selectable.ok ? undefined : selectable.reason}
                >
                    <span className="flex min-w-0 items-center gap-1.5">
                        <span className="truncate text-[13px] font-medium text-text">
                            {info.label}
                        </span>
                    </span>
                    <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-2xs">
                        <StatusDot tone={TONE_DOT[status.tone]} size={6} className="shrink-0" />
                        <span className={cn('truncate', TONE_TEXT[status.tone])}>
                            {status.text}
                        </span>
                    </span>
                    {(info.endpoint || info.token_hint) && (
                        <span className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-2 font-mono text-[10.5px] text-text-tertiary">
                            {info.endpoint && <span className="truncate">{info.endpoint}</span>}
                            {info.token_hint && (
                                <span className="shrink-0">token {info.token_hint}</span>
                            )}
                        </span>
                    )}
                    <span className="mt-1.5 flex flex-wrap items-center gap-1">
                        <CapabilityChip on={info.can_call} label="可调用" />
                        <CapabilityChip on={info.can_receive} label="可收事件" />
                    </span>
                    {!selectable.ok && selectable.reason && (
                        <span className="mt-1 block text-2xs text-text-secondary">
                            {selectable.reason}
                        </span>
                    )}
                </OptionButton>
                <button
                    type="button"
                    onClick={runTest}
                    disabled={!canTest || test.isPending}
                    title={
                        canTest
                            ? '测一下这条通道现在通不通'
                            : running
                              ? '这条通道不支持'
                              : 'Bot 没在运行，启动后再测'
                    }
                    className={cn(
                        'mr-1 mt-1.5 inline-flex h-6 shrink-0 items-center gap-1 rounded-xs px-1.5 text-2xs font-medium text-text-secondary transition-colors',
                        'hover:bg-brand-soft hover:text-brand focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent disabled:hover:text-text-secondary',
                    )}
                >
                    {test.isPending ? <Spinner size="xs" tone="brand" label="正在测试" /> : null}
                    {test.isPending ? '测试中' : '测试连通'}
                </button>
            </div>
            {/* 测试结果放在选项按钮外面：不并进它的可访问名，点它也不会误选这一行 */}
            {result && (
                <p
                    role="status"
                    className={cn(
                        'pb-2 pl-8 pr-2 text-2xs',
                        result.kind === 'failed'
                            ? 'text-danger'
                            : TONE_TEXT[channelStatusCopy(result.status).tone],
                    )}
                >
                    {result.kind === 'failed'
                        ? '测试没跑成，原因见上方提示条'
                        : `测完了：${channelStatusCopy(result.status).text}`}
                </p>
            )}
        </div>
    );
}

function CapabilityChip({ on, label }: { on: boolean; label: string }) {
    return (
        <span
            className={cn(
                'inline-flex h-4 items-center rounded-xs px-1 text-[10px] font-medium leading-none',
                on
                    ? 'bg-success-soft text-success'
                    : 'bg-inset text-text-disabled line-through decoration-text-disabled/60',
            )}
        >
            {label}
        </span>
    );
}

/** 通道列表还在读：和真实的通道行差不多高的骨架，读到后弹层不跳 */
function ChannelSkeleton() {
    return (
        <div role="status" aria-label="正在读取通道" className="flex flex-col gap-1 p-1">
            {[0, 1, 2].map((i) => (
                <div key={i} className="flex items-start gap-2.5 rounded-sm px-2 py-2">
                    <Shimmer height={7} className="mt-1 w-[7px] shrink-0 !rounded-full" />
                    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                        <Shimmer height={11} className={i === 0 ? 'w-28' : 'w-40'} />
                        <Shimmer height={9} className="w-52 opacity-70" />
                    </div>
                </div>
            ))}
        </div>
    );
}
