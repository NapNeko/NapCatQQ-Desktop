import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Virtualizer } from '@tanstack/react-virtual';
import type { WheelEvent } from 'react';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Message } from '../../core/domain/chat/model';
import { readTimelinePosition, useTimelinePosition } from './useTimelinePosition';
import { useStickToBottom } from '../debug/right/useStickToBottom';
import type { ChatReadingPosition } from '../../core/ipc/generated/chat/ChatReadingPosition';

const message: Message = {
    key: 'private:22/42',
    id: '42',
    session: 'private:22',
    senderId: '22',
    senderName: '朋友',
    at: 0,
    mine: false,
    segments: [],
    status: 'sent',
};
const handoff = (store: ChatAccountStore, position: ChatReadingPosition) =>
    store.restoreView({
        ...store.view(),
        active: message.session,
        reading: { [message.session]: position },
    });
function setup() {
    const store = new ChatAccountStore(
        {
            bot_id: 'position-test',
            qq_id: 99,
            name: '测试',
            backend: 'napcat',
            host: { kind: 'local' },
            running: false,
            online: false,
        },
        { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
    );
    const element = document.createElement('div');
    Object.defineProperties(element, {
        clientWidth: { value: 480 },
        clientHeight: { value: 300 },
        scrollHeight: { value: 2400, configurable: true },
    });
    const rows = [{ index: 0, key: message.key, start: 1200, end: 1400, size: 200, lane: 0 }];
    const virtual = {
        measurementsCache: rows,
        getVirtualItemForOffset: (top: number) => rows.find((row) => row.end > top),
        getOffsetForIndex: (index: number) => [rows[index].start, 'start'],
        scrollToOffset: vi.fn((top: number) => {
            element.scrollTop = top;
        }),
        scrollToIndex: vi.fn(),
        options: { count: 1 },
    } as unknown as Virtualizer<HTMLDivElement, Element>;
    return { store, element, rows, virtual };
}
describe('chat reading position', () => {
    it('keeps the same message offset when older working pages are removed', () => {
        const { store, element, rows, virtual } = setup();
        const earlier = { ...message, key: 'private:22/1', id: '1', at: -1 };
        const anchorRow = { ...rows[0], index: 1 };
        rows[0] = { ...rows[0], key: earlier.key, start: 0, end: 1200, size: 1200 };
        rows.push(anchorRow);
        const scroll = { current: element };
        element.scrollTop = 1231;
        store.readingPosition(message.session, {
            messageKey: message.key,
            messageId: '42',
            offset: 31,
            atBottom: false,
        });
        const hook = renderHook(
            ({ messages }) =>
                useTimelinePosition(store, message.session, scroll, virtual, messages, () => false),
            {
                initialProps: { messages: [earlier, message] },
            },
        );
        rows.splice(0, 2, { ...anchorRow, index: 0, start: 900, end: 1100 });
        hook.rerender({ messages: [message] });
        expect(element.scrollTop).toBe(931);
    });
    it('captures a message and its partial-row offset rather than a window-specific pixel', () => {
        const { element, virtual } = setup();
        element.scrollTop = 1231;
        expect(readTimelinePosition(virtual, element, [message], false)).toEqual({
            messageKey: message.key,
            messageId: '42',
            offset: 31,
            atBottom: false,
        });
    });
    it('waits for the anchor to mount and uses its measured position in the new window', () => {
        const { store, element, rows, virtual } = setup();
        store.scroll(message.session, 6431);
        handoff(store, { messageKey: message.key, messageId: '42', offset: 31, atBottom: false });
        const hook = renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => false,
            ),
        );
        expect(element.scrollTop).toBe(1231);
        expect(hook.result.current.restoring()).toBe(true);
        const node = document.createElement('div');
        node.dataset.messageKey = message.key;
        element.append(node);
        rows[0] = { ...rows[0], start: 950, end: 1150 };
        hook.rerender();
        expect(element.scrollTop).toBe(981);
        expect(hook.result.current.restoring()).toBe(false);
        expect(store.view().reading[message.session].offset).toBe(31);
        expect(store.initialReadingPosition(message.session)).toBeUndefined();
    });
    it('lets a user scroll cancel an unfinished restoration', () => {
        const { store, element, virtual } = setup();
        handoff(store, { messageKey: 'not-loaded', messageId: null, offset: 31, atBottom: false });
        const hook = renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => false,
            ),
        );
        act(() => hook.result.current.cancelRestore());
        element.scrollTop = 1250;
        hook.rerender();
        expect(virtual.scrollToOffset).not.toHaveBeenCalled();
        expect(store.view().reading[message.session].messageKey).toBe(message.key);
    });
    it('does not restore an old pixel when the previous window was following the latest message', () => {
        const { store, element, virtual } = setup();
        store.scroll(message.session, 100);
        handoff(store, { messageKey: message.key, messageId: '42', offset: 0, atBottom: true });
        renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => true,
            ),
        );
        expect(virtual.scrollToOffset).not.toHaveBeenCalled();
    });
    it('does not restore a past reading position on an ordinary conversation entry', () => {
        const { store, element, virtual } = setup();
        store.scroll(message.session, 1400);
        store.readingPosition(message.session, {
            messageKey: message.key,
            messageId: '42',
            offset: 31,
            atBottom: false,
        });
        const hook = renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => true,
            ),
        );
        expect(hook.result.current.restoring()).toBe(false);
        expect(virtual.scrollToOffset).not.toHaveBeenCalled();
    });
    it('consumes the window handoff position once and does not restore it after switching back', () => {
        const { store, element, virtual } = setup();
        handoff(store, { messageKey: message.key, messageId: '42', offset: 31, atBottom: false });
        const node = document.createElement('div');
        node.dataset.messageKey = message.key;
        element.append(node);
        const first = renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => false,
            ),
        );
        expect(element.scrollTop).toBe(1231);
        first.unmount();
        vi.mocked(virtual.scrollToOffset).mockClear();
        const next = renderHook(() =>
            useTimelinePosition(
                store,
                message.session,
                { current: element },
                virtual,
                [message],
                () => true,
            ),
        );
        expect(next.result.current.restoring()).toBe(false);
        expect(virtual.scrollToOffset).not.toHaveBeenCalled();
    });
    it('does not reattach after a programmatic adjustment moves a detached reader near the bottom', () => {
        const { element, virtual } = setup();
        element.scrollTop = 1200;
        const hook = renderHook(() =>
            useStickToBottom({
                scrollRef: { current: element },
                virtualizer: virtual,
                items: [message],
                memoryKey: null,
                resetToken: 'test',
                filterToken: '',
                animate: false,
                initialDetached: true,
                reattachOnIntent: true,
            }),
        );
        element.scrollTop = 2100;
        act(() => hook.result.current.handlers.onScroll());
        expect(hook.result.current.isFollowing()).toBe(false);
        act(() => {
            hook.result.current.handlers.onWheel({ deltaY: 120 } as WheelEvent<HTMLElement>);
            hook.result.current.handlers.onScroll();
        });
        expect(hook.result.current.isFollowing()).toBe(true);
    });
    it('keeps a newly opened conversation pinned through delayed layout changes until the user scrolls upward', () => {
        const { element, virtual } = setup();
        let height = 2400;
        Object.defineProperty(element, 'scrollHeight', { configurable: true, get: () => height });
        const scrollRef = { current: element };
        const hook = renderHook(() =>
            useStickToBottom({
                scrollRef,
                virtualizer: virtual,
                items: [message],
                memoryKey: null,
                resetToken: 'test',
                filterToken: '',
                animate: false,
                reattachOnIntent: true,
                followUntilUserScroll: true,
            }),
        );
        expect(element.scrollTop).toBe(2100);
        expect(virtual.scrollToIndex).not.toHaveBeenCalled();
        height += 200;
        act(() => hook.result.current.handlers.onScroll());
        expect(hook.result.current.isFollowing()).toBe(true);
        hook.rerender();
        expect(element.scrollTop).toBe(2300);
        act(() =>
            hook.result.current.handlers.onWheel({ deltaY: -120 } as WheelEvent<HTMLElement>),
        );
        element.scrollTop = 2180;
        height += 100;
        hook.rerender();
        expect(hook.result.current.isFollowing()).toBe(false);
        expect(element.scrollTop).toBe(2180);
    });
});
