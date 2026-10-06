import { beforeEach, describe, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({
    invoke: vi.fn(),
    listeners: [] as {
        event: string;
        target?: string;
        callback: (event: { payload: unknown }) => void;
    }[],
}));
vi.mock('../ipc/transport', async (original) => ({
    ...(await original<typeof import('../ipc/transport')>()),
    isTauri: true,
}));
vi.mock('@tauri-apps/api/core', async (original) => ({
    ...(await original<typeof import('@tauri-apps/api/core')>()),
    invoke: native.invoke,
}));
vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(
        async (
            event: string,
            callback: (event: { payload: unknown }) => void,
            options?: { target?: string },
        ) => {
            const entry = { event, callback, target: options?.target };
            native.listeners.push(entry);
            return () => {
                native.listeners = native.listeners.filter((listener) => listener !== entry);
            };
        },
    ),
}));
beforeEach(() => {
    native.listeners = [];
    native.invoke.mockReset();
    vi.resetModules();
});
const savedView = () => ({
    v: 1,
    revision: 2,
    selectedBot: 'bot',
    accounts: [
        {
            botId: 'bot',
            selfId: '99',
            active: 'private:22',
            drafts: { 'private:22': { text: '草稿', attachments: [], reply: null, mentions: [] } },
            reading: {
                'private:22': {
                    messageKey: 'private:22/42',
                    messageId: '42',
                    offset: 31,
                    atBottom: false,
                },
            },
            scroll: { 'private:22': 800 },
        },
    ],
});
function emitTo(target: string, event: string, payload: unknown) {
    for (const listener of native.listeners)
        if (listener.event === event && (!listener.target || listener.target === target))
            listener.callback({ payload: JSON.stringify(payload) });
}
describe('chat window event routing', () => {
    it('keeps local ignore and hidden groups as separate account-scoped mutations', async () => {
        const { chatDesktopService } = await import('./chat-desktop.service');
        await chatDesktopService.ignoreGroup('bot-1', '99', '123', true);
        await chatDesktopService.ignoreGroup('bot-1', '99', '123', false, true);
        expect(native.invoke).toHaveBeenNthCalledWith(1, 'chat_set_group_ignored', {
            botId: 'bot-1',
            selfId: '99',
            groupId: '123',
            ignored: true,
            hidden: false,
        });
        expect(native.invoke).toHaveBeenNthCalledWith(2, 'chat_set_group_ignored', {
            botId: 'bot-1',
            selfId: '99',
            groupId: '123',
            ignored: false,
            hidden: true,
        });
    });
    it('updates native account icon selection only from the chat window', async () => {
        const { chatDesktopService, markChatPopoutWindow } = await import('./chat-desktop.service');
        await chatDesktopService.selectAccount('main-choice');
        expect(native.invoke).not.toHaveBeenCalled();
        markChatPopoutWindow();
        await chatDesktopService.selectAccount('chat-choice');
        expect(native.invoke).toHaveBeenCalledWith('chat_select_account', { botId: 'chat-choice' });
    });
    it('opens an ordinary chat page at the latest message while preserving its active conversation and draft', async () => {
        const { chatDesktopService } = await import('./chat-desktop.service');
        const source = savedView();
        native.invoke.mockResolvedValue(source);
        const view = await chatDesktopService.loadView(true);
        expect(view.accounts[0].active).toBe('private:22');
        expect(view.accounts[0].drafts['private:22'].text).toBe('草稿');
        expect(view.accounts[0].reading).toEqual({});
        expect(view.accounts[0].scroll).toEqual({});
        expect(source.accounts[0].reading['private:22'].messageId).toBe('42');
    });
    it('restores a newly detached window once and forgets the old position on a later page entry', async () => {
        const { chatDesktopService, markChatPopoutWindow } = await import('./chat-desktop.service');
        native.invoke.mockResolvedValue(savedView());
        markChatPopoutWindow();
        expect(
            (await chatDesktopService.loadView(true)).accounts[0].reading['private:22'].offset,
        ).toBe(31);
        expect((await chatDesktopService.loadView(true)).accounts[0].reading).toEqual({});
    });
    it('restores the reading position when the popout is embedded back into the main window', async () => {
        const { chatDesktopService } = await import('./chat-desktop.service');
        native.invoke.mockResolvedValue(savedView());
        await chatDesktopService.onEmbedRequested(vi.fn());
        emitTo('main', 'chat-embed-requested', { v: 1 });
        expect(
            (await chatDesktopService.loadView(true)).accounts[0].reading['private:22'].offset,
        ).toBe(31);
    });
    it('recovers an embed request when the main webview is created after the event', async () => {
        const { chatDesktopService } = await import('./chat-desktop.service');
        native.invoke
            .mockResolvedValueOnce({ v: 1, detached: false, embedRequested: true })
            .mockResolvedValueOnce(savedView());
        await chatDesktopService.windowState();
        expect(
            (await chatDesktopService.loadView(true)).accounts[0].reading['private:22'].offset,
        ).toBe(31);
    });
    it('delivers a handoff only to its owning window and removes its listener', async () => {
        const { chatDesktopService, markChatPopoutWindow } = await import('./chat-desktop.service');
        const main = vi.fn();
        const panel = vi.fn();
        const unlistenMain = await chatDesktopService.onRequest(main);
        markChatPopoutWindow();
        await chatDesktopService.onRequest(panel);
        const request = { v: 1, requestId: 'handoff', action: 'close' };
        emitTo('chat-panel', 'chat-window-request', request);
        expect(panel).toHaveBeenCalledWith(request);
        expect(main).not.toHaveBeenCalled();
        panel.mockClear();
        emitTo('main', 'chat-window-request', { ...request, action: 'popout' });
        expect(main).toHaveBeenCalledOnce();
        expect(panel).not.toHaveBeenCalled();
        unlistenMain();
        main.mockClear();
        emitTo('main', 'chat-window-request', request);
        expect(main).not.toHaveBeenCalled();
    });
    it('keeps account selection in the popout and embed requests in the main window', async () => {
        const { chatDesktopService, markChatPopoutWindow } = await import('./chat-desktop.service');
        const selected = vi.fn();
        const embedded = vi.fn();
        await chatDesktopService.onEmbedRequested(embedded);
        markChatPopoutWindow();
        await chatDesktopService.onAccountSelected(selected);
        emitTo('main', 'chat-account-selected', { v: 1 });
        emitTo('chat-panel', 'chat-embed-requested', { v: 1 });
        expect(selected).not.toHaveBeenCalled();
        expect(embedded).not.toHaveBeenCalled();
        emitTo('chat-panel', 'chat-account-selected', { v: 1 });
        emitTo('main', 'chat-embed-requested', { v: 1 });
        expect(selected).toHaveBeenCalledOnce();
        expect(embedded).toHaveBeenCalledOnce();
    });
});
