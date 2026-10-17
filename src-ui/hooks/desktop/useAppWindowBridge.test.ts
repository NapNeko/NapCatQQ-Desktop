import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChatWindowRequest } from '../../core/ipc/generated/chat/ChatWindowRequest';
import { useAppWindowBridge } from './useAppWindowBridge';

const bridge = vi.hoisted(() => ({
    onRequest: vi.fn(),
    reply: vi.fn(),
    prepare: vi.fn(),
    invalidateQueries: vi.fn(),
}));
vi.mock('@tanstack/react-query', () => ({ useQueryClient: () => bridge }));
vi.mock('../../core/services/chat-desktop.service', () => ({
    chatDesktopService: {
        onRequest: bridge.onRequest,
        reply: bridge.reply,
        onEmbedRequested: async () => () => {},
        windowState: async () => ({ embedRequested: false }),
    },
}));
vi.mock('../../core/services/debug-window.service', () => ({ debugWindowService: {} }));
vi.mock('../../core/services/desktop.service', () => ({
    windowEventService: { onDebugPopoutClosed: async () => () => {} },
}));
vi.mock('../debug/debugWorkspaceStore', () => ({ markWorkspaceStale: vi.fn() }));
vi.mock('../preferences/featureTogglesStore', () => ({
    featureTogglesStore: { getSnapshot: () => ({ chat: true }) },
}));
vi.mock('../chat/chatStore', () => ({
    prepareChatHandoff: bridge.prepare,
    getChatSelectedBot: () => 'bot',
}));

let receive: (request: ChatWindowRequest) => void;
beforeEach(() => {
    vi.clearAllMocks();
    bridge.prepare.mockReset().mockResolvedValue(undefined);
    bridge.reply.mockResolvedValue(undefined);
    bridge.onRequest.mockImplementation(async (cb) => {
        receive = cb;
        return vi.fn();
    });
});
afterEach(cleanup);

describe('main window chat handoff recovery', () => {
    it('restores the route only after the canceled draft cleanup has finished', async () => {
        let fail!: (error: Error) => void;
        let lateUnmount!: () => void;
        bridge.prepare.mockImplementationOnce((_bot, unmount) => {
            lateUnmount = unmount;
            unmount();
            return new Promise<void>((_resolve, reject) => {
                fail = reject;
            });
        });
        const showChat = vi.fn();
        const showOverview = vi.fn();
        renderHook(() =>
            useAppWindowBridge({ route: 'chat', showChat, showOverview, showChatQuiet: vi.fn() }),
        );
        act(() => receive({ v: 1, requestId: 'old', action: 'popout' }));
        await waitFor(() => expect(bridge.prepare).toHaveBeenCalledOnce());
        act(() => receive({ v: 1, requestId: '', action: 'resume' }));
        expect(showChat).not.toHaveBeenCalled();
        act(() => lateUnmount());
        expect(showOverview).toHaveBeenCalledOnce();
        await act(async () => fail(new Error('聊天交接已取消')));
        expect(showChat).toHaveBeenCalledOnce();
        expect(bridge.reply).not.toHaveBeenCalled();
    });

    it('waits for the canceled save before starting another handoff', async () => {
        let finish!: () => void;
        bridge.prepare.mockImplementationOnce(
            () =>
                new Promise<void>((resolve) => {
                    finish = resolve;
                }),
        );
        renderHook(() =>
            useAppWindowBridge({
                route: 'chat',
                showChat: vi.fn(),
                showOverview: vi.fn(),
                showChatQuiet: vi.fn(),
            }),
        );
        act(() => receive({ v: 1, requestId: 'old', action: 'popout' }));
        await waitFor(() => expect(bridge.prepare).toHaveBeenCalledOnce());
        act(() => {
            receive({ v: 1, requestId: '', action: 'resume' });
            receive({ v: 1, requestId: 'new', action: 'release' });
        });
        expect(bridge.prepare).toHaveBeenCalledOnce();
        await act(async () => finish());
        await waitFor(() => expect(bridge.reply).toHaveBeenCalledWith('new', null));
        expect(bridge.prepare).toHaveBeenCalledTimes(2);
        expect(bridge.prepare).toHaveBeenLastCalledWith('bot', expect.any(Function), false);
        expect(bridge.reply).toHaveBeenCalledOnce();
    });

    it('still restores the chat route and reports an active handoff failure', async () => {
        bridge.prepare.mockRejectedValueOnce(new Error('消息仍在发送'));
        const showChat = vi.fn();
        renderHook(() =>
            useAppWindowBridge({
                route: 'chat',
                showChat,
                showOverview: vi.fn(),
                showChatQuiet: vi.fn(),
            }),
        );
        act(() => receive({ v: 1, requestId: 'current', action: 'popout' }));
        await waitFor(() =>
            expect(bridge.reply).toHaveBeenCalledWith('current', 'Error: 消息仍在发送'),
        );
        expect(showChat).toHaveBeenCalledOnce();
    });
});
