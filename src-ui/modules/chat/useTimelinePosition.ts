// 跨窗口按消息锚点恢复阅读位置，行高估算和窗口宽度不参与交接。
import { useCallback, useLayoutEffect, useRef, type RefObject } from 'react';
import type { Virtualizer } from '@tanstack/react-virtual';
import type { Message, SessionKey } from '../../core/domain/chat/model';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { ChatReadingPosition } from '../../core/ipc/generated/chat/ChatReadingPosition';

export function readTimelinePosition(virtual: Virtualizer<HTMLDivElement, Element>, element: HTMLDivElement, messages: readonly Message[], atBottom: boolean): ChatReadingPosition | undefined {
    const row = virtual.getVirtualItemForOffset(element.scrollTop);
    const message = row && messages[row.index];
    if (!message || !row) return;
    return { messageKey: message.key, messageId: message.id ?? null, offset: element.scrollTop - row.start, atBottom };
}

export function useTimelinePosition(store: ChatAccountStore, session: SessionKey, scroll: RefObject<HTMLDivElement>, virtual: Virtualizer<HTMLDivElement, Element>, messages: readonly Message[], isFollowing: () => boolean) {
    const pending = useRef(store.initialReadingPosition(session));
    const current = useRef({ messages, isFollowing }); current.current = { messages, isFollowing };
    const capture = useCallback(() => {
        const element = scroll.current;
        if (!element || !element.clientHeight || !element.clientWidth || pending.current) return;
        const value = readTimelinePosition(virtual, element, current.current.messages, current.current.isFollowing());
        if (value) { store.scroll(session, element.scrollTop); store.readingPosition(session, value); }
        return value;
    }, [store, session, scroll, virtual]);
    useLayoutEffect(() => store.captureTimeline(session, capture), [store, session, capture]);
    useLayoutEffect(() => {
        const element = scroll.current;
        if (!element?.clientHeight || !element.clientWidth || !messages.length) return;
        const saved = pending.current;
        if (saved?.atBottom) { store.finishInitialReading(session, saved); pending.current = undefined; return; }
        if (saved) {
            const index = messages.findIndex(message => message.key === saved.messageKey || !!saved.messageId && message.id === saved.messageId);
            if (index < 0) return;
            virtual.getOffsetForIndex(index, 'start');
            const row = virtual.measurementsCache[index];
            if (!row) return;
            virtual.scrollToOffset(Math.max(0, row.start + saved.offset));
            // 等锚点行实际挂载并测量后再接受新位置，不能把首帧估算写回交接数据。
            if (Array.from(element.querySelectorAll<HTMLElement>('[data-message-key]')).some(node => node.dataset.messageKey === messages[index].key)) { store.finishInitialReading(session, saved); pending.current = undefined; }
        }
    });
    const cancelRestore = useCallback(() => { if (pending.current) store.finishInitialReading(session, pending.current); pending.current = undefined; }, [store, session]);
    return { capture, cancelRestore, restoring: () => !!pending.current && !pending.current.atBottom };
}
