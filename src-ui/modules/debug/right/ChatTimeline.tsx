// 聊天时间线：虚拟列表（行高按实际量）、贴底跟随、「↓ N 条新消息」、时间分隔线、缓冲裁掉的提示。
//
// 行高不固定（文字几行、有没有图、有没有时间线），用 measureElement 实测；估算值只影响还没画过的行。
// 图片的框高度固定，加载完不改行高。anchorTo: 'end' 让缓冲满了从头丢条目时视口里的内容不跳。
//
// 刷屏时每帧都有新条目，virtualizer 每帧都要把没量过的行重新估一遍：估算按条目 key 缓存，
// 分隔线的「跨天」用整数算，不再每条 new 两个 Date。

import { memo, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { MAX_CHAT_ITEMS, type ChatItem } from '../../../core/domain/debug/chat';
import { needsTimeSeparator } from '../../../core/domain/debug/chatFormat';
import { ChatRow, type TimelineRow } from './ChatRow';
import { NewMessagesPill } from './NewMessagesPill';
import { useStickToBottom } from './useStickToBottom';

/** 同一个人连着发、间隔在这以内的，后面几条不再画头像和名字 */
const CONTINUE_MS = 3 * 60_000;
const SEPARATOR_PX = 28;
/** 头像、名字那一行和上边距：接着发的气泡没有它们 */
const MESSAGE_HEAD_PX = 22;
/** 跳到被回复的消息后闪多久；闪完清掉，行被卸了再挂上不会重闪 */
const FLASH_MS = 1500;

/** 不含分隔线、按「不是接着发的」算的高度；只和条目自己有关，所以能按 key 缓存 */
function baseEstimate(row: TimelineRow): number {
    switch (row.kind) {
        case 'message': {
            let h = 30 + MESSAGE_HEAD_PX;
            for (const s of row.segments) {
                if (s.type === 'image') h += 132;
                else if (s.type === 'mface') h += 100;
                else if (s.type === 'reply') h += 20;
                else if (s.type === 'file' || s.type === 'forward' || s.type === 'json' || s.type === 'xml') h += 56;
            }
            if (row.call) h += 16;
            return h;
        }
        case 'request':
            return 96;
        case 'gap':
        case 'dropped':
            return 32;
        default:
            return 28;
    }
}

/** 上一行是不是同一个人紧接着发的 */
function isContinuation(row: TimelineRow, prev: TimelineRow | undefined, showTime: boolean): boolean {
    if (showTime || row.kind !== 'message' || !prev || prev.kind !== 'message') return false;
    return (
        prev.senderId === row.senderId &&
        prev.direction === row.direction &&
        prev.session === row.session &&
        row.at - prev.at < CONTINUE_MS
    );
}

function rowAt(row: TimelineRow | undefined): number | undefined {
    return row && row.kind !== 'trimmed' ? row.at : undefined;
}

function showTimeFor(row: TimelineRow, prev: TimelineRow | undefined): boolean {
    return row.kind !== 'trimmed' && needsTimeSeparator(rowAt(prev), row.at);
}

export interface ChatTimelineProps {
    botId: string;
    items: readonly ChatItem[];
    selectedKey: string | null;
    /** 「全部」视图：气泡上带会话名 */
    showSessionName: boolean;
    /** 缓冲里更早被丢掉的条数；0 不显示提示 */
    trimmed: number;
    /** 换会话、暂停后继续：回到底部 */
    resetToken: string;
    filterToken: string;
    paused: boolean;
    /** 列表空着时画什么 */
    empty?: ReactNode;
    /** 暂停时底部的提示条（代替「新消息」胶囊） */
    pausedBar?: ReactNode;
    /** 右栏拿去实现「跳到被回复的消息」 */
    onRevealReady?: (reveal: ((messageId: number) => boolean) | null) => void;
    /** 右栏拿去在发出消息后回到最新 */
    onJumpReady?: (jump: (() => void) | null) => void;
}

export const ChatTimeline = memo(function ChatTimeline({
    botId,
    items,
    selectedKey,
    showSessionName,
    trimmed,
    resetToken,
    filterToken,
    paused,
    empty,
    pausedBar,
    onRevealReady,
    onJumpReady,
}: ChatTimelineProps) {
    const m = useMotion();
    const scrollRef = useRef<HTMLDivElement>(null);

    const rows = useMemo<TimelineRow[]>(
        () => (trimmed > 0 ? [{ kind: 'trimmed', key: 'trimmed-note', count: trimmed }, ...items] : (items as ChatItem[])),
        [items, trimmed],
    );
    const rowsRef = useRef(rows);
    rowsRef.current = rows;

    // 估算与 key：读 ref，函数本身不跟着每帧的新数组换。追加 / 从头裁掉时 count 或首尾 key 会变，
    // virtualizer 自己会重算；换会话、改筛选时列表整个换了，才换一个 getItemKey 让它从头来
    const estimates = useRef(new Map<string, number>());
    const estimateSize = useCallback((i: number) => {
        const list = rowsRef.current;
        const row = list[i];
        if (!row) return 40;
        let base = estimates.current.get(row.key);
        if (base === undefined) {
            if (estimates.current.size > MAX_CHAT_ITEMS * 2) estimates.current.clear();
            base = baseEstimate(row);
            estimates.current.set(row.key, base);
        }
        const prev = list[i - 1];
        const showTime = showTimeFor(row, prev);
        if (showTime) return base + SEPARATOR_PX;
        return isContinuation(row, prev, false) ? base - MESSAGE_HEAD_PX : base;
    }, []);
    const getItemKey = useCallback(
        (i: number) => rowsRef.current[i]?.key ?? i,
        // 换了一份列表（会话、筛选、暂停继续、缓冲提示出现）才换
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [resetToken, filterToken, trimmed > 0],
    );

    const virtualizer = useVirtualizer({
        count: rows.length,
        getScrollElement: () => scrollRef.current,
        estimateSize,
        getItemKey,
        overscan: 8,
        paddingStart: 4,
        paddingEnd: 14,
        anchorTo: 'end',
    });

    const stick = useStickToBottom({
        scrollRef,
        virtualizer,
        items: rows,
        memoryKey: `chat:${botId}`,
        resetToken,
        filterToken,
        animate: m.enabled && !paused,
    });

    // ---- 跳到被回复的消息，闪一下；闪完清掉
    const [flash, setFlash] = useState<{ key: string; n: number } | null>(null);
    useEffect(() => {
        if (!flash) return;
        const t = setTimeout(() => setFlash(null), FLASH_MS);
        return () => clearTimeout(t);
    }, [flash]);
    const { detach, jumpToLatest } = stick;
    useEffect(() => {
        if (!onRevealReady) return;
        onRevealReady((messageId) => {
            const list = rowsRef.current;
            let index = -1;
            for (let i = list.length - 1; i >= 0; i -= 1) {
                const r = list[i];
                if (r?.kind === 'message' && r.messageId === messageId) {
                    index = i;
                    break;
                }
            }
            const row = list[index];
            if (!row) return false;
            detach();
            // 直接跳过去再闪一下：virtualizer 的平滑滚动途中不量路过的行，远距离滚会叠行
            virtualizer.scrollToIndex(index, { align: 'center' });
            setFlash((f) => ({ key: row.key, n: (f?.n ?? 0) + 1 }));
            return true;
        });
        return () => onRevealReady(null);
    }, [onRevealReady, virtualizer, detach]);

    const smooth = m.enabled;
    useEffect(() => {
        if (!onJumpReady) return;
        onJumpReady(() => jumpToLatest(smooth));
        return () => onJumpReady(null);
    }, [onJumpReady, jumpToLatest, smooth]);

    const virtualItems = virtualizer.getVirtualItems();

    return (
        <div className="relative flex min-h-0 flex-1 flex-col">
            <div
                ref={scrollRef}
                role="log"
                aria-label="聊天记录"
                aria-live="off"
                data-testid="chat-scroller"
                {...stick.handlers}
                // 浏览器自己的滚动锚定会和虚拟列表的位置修正打架
                className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden overscroll-contain [overflow-anchor:none]"
            >
                <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
                    {virtualItems.map((v) => {
                        const row = rows[v.index];
                        if (!row) return null;
                        const prev = rows[v.index - 1];
                        const showTime = showTimeFor(row, prev);
                        return (
                            <div
                                key={row.key}
                                data-index={v.index}
                                ref={virtualizer.measureElement}
                                className="absolute left-0 top-0 w-full"
                                style={{ transform: `translateY(${v.start}px)` }}
                            >
                                <ChatRow
                                    row={row}
                                    showTime={showTime}
                                    continued={isContinuation(row, prev, showTime)}
                                    selected={row.key === selectedKey}
                                    showSessionName={showSessionName}
                                    enter={v.index >= stick.enterFrom}
                                    takeEnter={stick.takeEnter}
                                    flash={flash?.key === row.key ? flash.n : 0}
                                />
                            </div>
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
                <NewMessagesPill count={stick.unseen} away={stick.away} onClick={() => stick.jumpToLatest(m.enabled)} />
            )}
        </div>
    );
});
