// 「列表」模式：给要看原始事件的人的紧凑行，一行 24px：`HH:mm:ss.SSS  post_type/detail  摘要`。
// 和聊天视图用同一份筛选、同一个暂停；点一行看原始 JSON。行高固定，不用量。

import { memo, useRef, type ReactNode } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { cn } from '../../../shared/utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import type { ChatItem } from '../../../core/domain/debug/chat';
import { useChatView } from './chatContext';
import {
    clockTimeMs,
    countFormat,
    listRowOf,
    type RowTone,
} from '../../../core/domain/debug/chatFormat';
import { NewMessagesPill } from './NewMessagesPill';
import { useStickToBottom } from './useStickToBottom';

const ROW_PX = 24;

const TONE_CLASS: Record<RowTone, string> = {
    message: 'text-info',
    notice: 'text-text-secondary',
    request: 'text-brand',
    call: 'text-success',
    meta: 'text-text-tertiary',
    warn: 'text-warning',
    danger: 'text-danger',
    muted: 'text-text-tertiary',
};

export interface EventListViewProps {
    botId: string;
    items: readonly ChatItem[];
    /** 当前打开详情的那一条，高亮 */
    activeKey: string | null;
    trimmed: number;
    resetToken: string;
    filterToken: string;
    paused: boolean;
    empty?: ReactNode;
    pausedBar?: ReactNode;
}

export const EventListView = memo(function EventListView({
    botId,
    items,
    activeKey,
    trimmed,
    resetToken,
    filterToken,
    paused,
    empty,
    pausedBar,
}: EventListViewProps) {
    const m = useMotion();
    const api = useChatView();
    const scrollRef = useRef<HTMLDivElement>(null);
    const virtualizer = useVirtualizer({
        count: items.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => ROW_PX,
        getItemKey: (i) => items[i]?.key ?? i,
        overscan: 12,
        anchorTo: 'end',
    });
    const stick = useStickToBottom({
        scrollRef,
        virtualizer,
        items,
        memoryKey: `chat-list:${botId}`,
        resetToken,
        filterToken,
        animate: false,
    });

    return (
        <div className="relative flex min-h-0 flex-1 flex-col">
            {trimmed > 0 && (
                <div className="shrink-0 border-b border-dashed border-border px-3 py-1 text-center text-2xs text-text-tertiary">
                    更早的 {countFormat.format(trimmed)} 条已丢弃
                </div>
            )}
            <div
                ref={scrollRef}
                role="log"
                aria-label="事件列表"
                aria-live="off"
                {...stick.handlers}
                className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden overscroll-contain bg-inset/30 font-mono text-[11.5px] [overflow-anchor:none]"
            >
                <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
                    {virtualizer.getVirtualItems().map((v) => {
                        const item = items[v.index];
                        if (!item) return null;
                        return (
                            <ListRowView
                                key={item.key}
                                item={item}
                                start={v.start}
                                active={item.key === activeKey}
                                onOpen={api.openDetail}
                            />
                        );
                    })}
                </div>
            </div>
            {items.length === 0 && empty && (
                <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-6">
                    <div className="pointer-events-auto">{empty}</div>
                </div>
            )}
            {paused ? (
                pausedBar
            ) : (
                <NewMessagesPill
                    count={stick.unseen}
                    away={stick.away}
                    onClick={() => stick.jumpToLatest(m.enabled)}
                />
            )}
        </div>
    );
});

const ListRowView = memo(function ListRowView({
    item,
    start,
    active,
    onOpen,
}: {
    item: ChatItem;
    start: number;
    active: boolean;
    onOpen: (item: ChatItem, el: HTMLElement) => void;
}) {
    let row: ReturnType<typeof listRowOf>;
    try {
        row = listRowOf(item);
    } catch {
        row = { type: item.kind, summary: '[这条显示不了]', tone: 'muted' };
    }
    return (
        <button
            type="button"
            onClick={(e) => onOpen(item, e.currentTarget)}
            title={row.summary}
            className={cn(
                'absolute left-0 top-0 flex w-full items-center gap-2 px-2 text-left transition-colors',
                'hover:bg-inset focus-visible:bg-inset focus-visible:outline-none',
                active && 'bg-brand-soft/70 hover:bg-brand-soft',
            )}
            style={{ height: ROW_PX, transform: `translateY(${start}px)` }}
        >
            <span className="w-[86px] shrink-0 tabular-nums text-text-tertiary">
                {clockTimeMs(item.at)}
            </span>
            <span className={cn('w-[128px] shrink-0 truncate', TONE_CLASS[row.tone])}>
                {row.type}
            </span>
            <span className="min-w-0 flex-1 truncate font-sans text-[12px] text-text-secondary">
                {row.summary}
            </span>
        </button>
    );
});
