// 一条事件的原始 JSON。整栏只有一个弹层，对着点的那个元素弹；
// 元素被虚拟列表卸掉（用户滚走了）时停在最后一次的位置，不会飞到左上角。
// 消息条目多一行快捷操作：回复 / 撤回 / 查发送者——在中栏开一个预填好的请求标签，发不发由中栏决定。

import { useMemo, useRef } from 'react';
import { Check, Copy, Reply, Undo2, UserRoundSearch, X } from 'lucide-react';
import { JsonTree, Popover, PopoverAnchor, PopoverContent } from '../../../shared/ui';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import type { ChatItem } from '../../../core/domain/debug/chat';
import {
    callLine,
    clockTimeMs,
    dayLabel,
    listRowOf,
    safeJson,
} from '../../../core/domain/debug/chatFormat';
import { messageLinkages, type EventLinkageId } from '../../../core/domain/debug/eventActions';
import { formatParams } from '../../../core/domain/debug/paramsText';
import { useCopy } from '../../../shared/chat/rightParts';

export interface DetailTarget {
    item: ChatItem;
    anchor: HTMLElement;
}

const EMPTY_RECT = {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    toJSON: () => ({}),
} as DOMRect;

const LINK_ICON: Record<EventLinkageId, typeof Reply> = {
    reply: Reply,
    recall: Undo2,
    sender: UserRoundSearch,
};

/** 弹层里看的是什么：有原始载荷看载荷，缺口之类没有载荷的看条目本身 */
function payloadOf(item: ChatItem): unknown {
    return 'raw' in item ? item.raw : item;
}

export function EventDetailPopover({
    target,
    onClose,
}: {
    target: DetailTarget | null;
    onClose: () => void;
}) {
    const { copied, copy } = useCopy();
    const lastRect = useRef<DOMRect | null>(null);
    const anchorRef = useRef<HTMLElement | null>(null);
    anchorRef.current = target?.anchor ?? null;
    const virtualRef = useRef({
        getBoundingClientRect: (): DOMRect => {
            const el = anchorRef.current;
            if (el && el.isConnected) lastRect.current = el.getBoundingClientRect();
            return lastRect.current ?? EMPTY_RECT;
        },
    });

    const item = target?.item ?? null;
    const value = useMemo(() => (item ? payloadOf(item) : null), [item]);
    const head = item ? listRowOf(item) : null;
    // 自己发的气泡被 message_sent 合并后，载荷是事件；调用结果单独写一行
    const call = item && item.kind === 'message' && item.call ? callLine(item.call) : null;
    const links = useMemo(
        () => (item && item.kind === 'message' ? messageLinkages(item) : []),
        [item],
    );

    /** 在中栏开一个预填好的标签；安全分级照旧，点开不等于发出 */
    const openLinkage = (action: string, params: Record<string, unknown>) => {
        debugWorkspaceStore.openAction(action, { newTab: true, paramsText: formatParams(params) });
        onClose();
    };

    return (
        <Popover open={!!target} onOpenChange={(open) => !open && onClose()}>
            <PopoverAnchor virtualRef={virtualRef} />
            <PopoverContent
                side="left"
                align="start"
                onOpenAutoFocus={(e) => e.preventDefault()}
                className="flex h-[min(460px,62vh)] w-[min(460px,calc(100vw-48px))] flex-col overflow-hidden p-0"
            >
                {item && head && (
                    <>
                        <div className="flex shrink-0 items-start gap-2 border-b border-border-subtle/70 px-3 py-2">
                            <div className="min-w-0 flex-1">
                                <p className="truncate font-mono text-[12px] font-semibold text-text">
                                    {head.type}
                                </p>
                                <p className="mt-0.5 truncate text-2xs tabular-nums text-text-tertiary">
                                    {dayLabel(item.at).replace(/ \d\d:\d\d$/, '')}{' '}
                                    {clockTimeMs(item.at)} · seq {item.seq}
                                </p>
                                {call && (
                                    <p
                                        className={`mt-0.5 truncate font-mono text-[10.5px] ${call.ok ? 'text-success' : 'text-danger'}`}
                                    >
                                        调用 {call.text}
                                    </p>
                                )}
                            </div>
                            <button
                                type="button"
                                onClick={() => copy(safeJson(value))}
                                className="inline-flex h-7 shrink-0 items-center gap-1 rounded-xs px-2 text-2xs font-medium text-text-secondary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                            >
                                {copied ? (
                                    <Check size={12} aria-hidden className="text-success" />
                                ) : (
                                    <Copy size={12} aria-hidden />
                                )}
                                {copied ? '已复制' : '复制 JSON'}
                            </button>
                            <button
                                type="button"
                                onClick={onClose}
                                aria-label="关闭"
                                className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-xs text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                            >
                                <X size={13} aria-hidden />
                            </button>
                        </div>
                        {links.length > 0 && (
                            <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-border-subtle/70 px-2 py-1">
                                {links.map((l) => {
                                    const Icon = LINK_ICON[l.id];
                                    return (
                                        <button
                                            key={l.id}
                                            type="button"
                                            onClick={() => openLinkage(l.action, l.params)}
                                            title={`在中栏开一个预填好的 ${l.action} 标签`}
                                            className="inline-flex h-7 items-center gap-1 rounded-xs px-2 text-2xs font-medium text-text-secondary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                        >
                                            <Icon size={12} aria-hidden />
                                            {l.label}
                                        </button>
                                    );
                                })}
                            </div>
                        )}
                        <JsonTree
                            key={item.key}
                            value={value}
                            defaultExpandDepth={2}
                            className="min-h-0 flex-1 px-1 py-1"
                        />
                    </>
                )}
            </PopoverContent>
        </Popover>
    );
}
