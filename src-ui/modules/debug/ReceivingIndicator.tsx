// 顶栏「正在接收 N 个 Bot」：离开调试台后事件还在后台收，这里让用户看得见、能逐个停。一个都没在收时不显示。

import { memo, useEffect, useMemo, useRef, useState } from 'react';
import { Square } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Popover, PopoverContent, PopoverTrigger, Spinner } from '../../shared/ui';
import { StatusDot } from '../../shared/ui/motion';
import { useDebugReceivers, useStopReceiver } from '../../hooks/debug/useDebugReceivers';
import { useDebugReceiverState } from '../../hooks/debug/debugEventStore';
import { channelShortLabel } from '../../core/domain/debug/channelCopy';
import { activeReceivers, receiverStateCopy } from '../../core/domain/debug/receiverCopy';
import { targetDisplayName } from '../../core/domain/debug/targetGroups';
import type { DebugReceiverInfo } from '../../core/ipc/generated/debug/DebugReceiverInfo';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';

const TONE_TEXT = {
    success: 'text-success',
    warning: 'text-warning',
    danger: 'text-danger',
    neutral: 'text-text-tertiary',
} as const;

const count = new Intl.NumberFormat('zh-CN');

export interface ReceivingIndicatorProps {
    targets: readonly DebugTarget[];
    /** 当前在看的 Bot：它开始 / 停止接收时马上刷新，不等 5 秒一次的轮询 */
    watchBotId: string | null;
}

export const ReceivingIndicator = memo(function ReceivingIndicator({ targets, watchBotId }: ReceivingIndicatorProps) {
    const [open, setOpen] = useState(false);
    const receivers = useDebugReceivers();
    const { refetch } = receivers;
    const watched = useDebugReceiverState(watchBotId);
    const watchedKey = `${watchBotId}|${watched.subscribed}|${watched.state?.state ?? ''}`;
    const lastWatchedKey = useRef(watchedKey);

    useEffect(() => {
        // 首次挂载时查询自己会拉，这里只管之后的变化
        if (lastWatchedKey.current === watchedKey) return;
        lastWatchedKey.current = watchedKey;
        void refetch();
    }, [watchedKey, refetch]);

    const active = useMemo(() => activeReceivers(receivers.data), [receivers.data]);
    const names = useMemo(() => new Map(targets.map((t) => [t.bot_id, targetDisplayName(t)])), [targets]);

    // 全停了就把弹出层也收起来，不然下一次冒出来时它会直接是打开的
    useEffect(() => {
        if (active.length === 0 && open) setOpen(false);
    }, [active.length, open]);

    if (active.length === 0) return null;

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    aria-label={`正在接收 ${active.length} 个 Bot 的事件，点击查看`}
                    className={cn(
                        'inline-flex h-8 shrink-0 items-center gap-2 rounded-pill border border-success/25 bg-success-soft/70 px-3 text-[12px] font-medium text-success',
                        'transition-colors hover:bg-success-soft data-[state=open]:bg-success-soft',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-success focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                    )}
                >
                    <StatusDot tone="running" size={7} />
                    <span aria-hidden className="whitespace-nowrap tabular-nums">
                        <span className="hidden @min-[760px]:inline">正在接收 {active.length} 个 Bot</span>
                        <span className="@min-[760px]:hidden">接收 {active.length}</span>
                    </span>
                </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="flex w-[340px] flex-col overflow-hidden p-0">
                <div className="border-b border-border-subtle/70 px-3 py-2.5">
                    <p className="font-display text-[13px] font-semibold text-text">正在接收事件</p>
                    <p className="mt-0.5 text-2xs leading-relaxed text-text-tertiary">
                        离开调试台后照样在后台收。Bot 停止、在这里停止、或者 30 分钟没人看也没人调用时会自己停。
                    </p>
                </div>
                <ul className="max-h-[min(360px,55vh)] overflow-y-auto p-1">
                    {active.map((r) => (
                        <ReceiverRow key={r.bot_id} info={r} name={names.get(r.bot_id) ?? r.bot_id} />
                    ))}
                </ul>
            </PopoverContent>
        </Popover>
    );
});

function ReceiverRow({ info, name }: { info: DebugReceiverInfo; name: string }) {
    // 每行一个 mutation：停一个时别的行的按钮不跟着转
    const stop = useStopReceiver();
    const state = receiverStateCopy(info.state);
    return (
        <li className="flex items-center gap-2.5 rounded-sm px-2 py-2 transition-colors hover:bg-inset/60">
            <StatusDot
                tone={info.state.state === 'connected' ? 'success' : info.state.state === 'reconnecting' ? 'warning' : 'idle'}
                size={7}
                className="shrink-0"
            />
            <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-baseline gap-1.5">
                    <span className="truncate text-[13px] font-medium text-text">{name}</span>
                    <span className="shrink-0 text-2xs text-text-tertiary">{channelShortLabel(info.source)}</span>
                </div>
                <div className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 text-2xs">
                    <span className={TONE_TEXT[state.tone]}>{state.text}</span>
                    <span aria-hidden className="text-border">
                        ·
                    </span>
                    <span className="tabular-nums text-text-tertiary">缓冲 {count.format(info.buffered)} 条</span>
                    {info.dropped_total > 0 && (
                        <>
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="tabular-nums text-warning">丢了 {count.format(info.dropped_total)} 条</span>
                        </>
                    )}
                </div>
            </div>
            <button
                type="button"
                onClick={() => stop.mutate(info.bot_id)}
                disabled={stop.isPending}
                aria-label={`停止接收 ${name} 的事件`}
                className={cn(
                    'inline-flex h-7 shrink-0 items-center gap-1 rounded-xs px-2 text-2xs font-medium text-text-secondary transition-colors',
                    'hover:bg-danger-soft hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger',
                    'disabled:cursor-not-allowed disabled:opacity-60',
                )}
            >
                {stop.isPending ? <Spinner size="xs" label="正在停止" /> : <Square size={10} strokeWidth={3} aria-hidden />}
                停止
            </button>
        </li>
    );
}
