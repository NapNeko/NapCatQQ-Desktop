import { createElement } from 'react';
import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
    captureChatScreenshot,
    cancelChatScreenshot,
    useChatScreenshot,
    useAppScreenshotShortcut,
} from './useChatScreenshot';
import { ChatAccountStore, prepareChatHandoff } from './chatStore';
import { ChatComposer } from '../../modules/chat/ChatComposer';
import { chatService } from '../../core/services/chat.service';
import { EMPTY_DRAFT, type Draft, type SessionKey } from '../../core/domain/chat/model';
import type { ChatScreenshotAttachment } from '../../core/ipc/generated/chat/ChatScreenshotAttachment';
import type { ChatScreenshotShortcutEvent } from '../../core/ipc/generated/chat/ChatScreenshotShortcutEvent';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';

const screenshot = vi.hoisted(() => ({
    capture: vi.fn(),
    cancel: vi.fn(),
    shortcut: vi.fn(),
    onShortcut: vi.fn(),
    notice: vi.fn(),
    handler: null as ((event: ChatScreenshotShortcutEvent) => void) | null,
}));
vi.mock('../../core/services/chat-screenshot.service', () => ({
    chatScreenshotService: screenshot,
}));
vi.mock('../ui/globalInfoBarStore', async (original) => ({
    ...(await original<typeof import('../ui/globalInfoBarStore')>()),
    pushInfoBar: screenshot.notice,
}));

const file: ChatScreenshotAttachment = {
    path: 'D:/cache/chat-screenshots/full.png',
    previewPath: 'D:/cache/chat-screenshots/preview.png',
    name: '截图.png',
    width: 1920,
    height: 1080,
};
const options = {
    addToChat: true,
    hideWindow: true,
    globalShortcut: true,
    shortcut: 'Ctrl+Alt+S',
};
const ok = (data: unknown): DebugCallResponse => ({
    request_id: 'request',
    result: {
        kind: 'ok',
        outcome: {
            ok: true,
            data,
            status: 'ok',
            retcode: 0,
            message: '',
            wording: '',
            raw: {},
            channel: { kind: 'internal' },
            elapsed_ms: 1,
            size_bytes: 0,
            truncated: false,
        },
    },
});
function setup() {
    const transport = {
        call: vi.fn(async (_bot: string, _action: string, _params: unknown) =>
            ok({ message_id: 42 }),
        ),
        subscribe: vi.fn(async () => ({
            subscription_id: 'screenshot-test',
            receiver: {
                bot_id: 'bot',
                state: { state: 'connected' as const },
                source: { kind: 'internal' as const },
                buffered: 0,
                first_seq: 1,
                viewers: 1,
                dropped_total: 0,
            },
        })),
        unsubscribe: vi.fn(async () => {}),
    };
    const store = new ChatAccountStore(
        {
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        },
        transport,
    );
    store.getSnapshot().account.active = 'private:12';
    store.draft('private:12', { ...EMPTY_DRAFT, text: '原草稿' });
    return { store, transport };
}
function pendingCapture() {
    let resolve!: (value: ChatScreenshotAttachment | null) => void;
    screenshot.capture.mockImplementationOnce(
        () =>
            new Promise((done) => {
                resolve = done;
            }),
    );
    return (value: ChatScreenshotAttachment | null = file) => resolve(value);
}
beforeEach(() => {
    vi.clearAllMocks();
    screenshot.capture.mockResolvedValue(file);
    screenshot.cancel.mockResolvedValue(undefined);
    screenshot.shortcut.mockResolvedValue(undefined);
    screenshot.onShortcut.mockImplementation(async (callback) => {
        screenshot.handler = callback;
        return () => {};
    });
});

describe('screenshot draft lifecycle', () => {
    it('registers the application shortcut before any chat composer mounts', async () => {
        const hook = renderHook(({ enabled }) => useAppScreenshotShortcut(enabled), {
            initialProps: { enabled: false },
        });
        expect(screenshot.shortcut).not.toHaveBeenCalled();
        hook.rerender({ enabled: true });
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true, global: true, context: '', key: 'S' }),
            ),
        );
        hook.unmount();
    });

    it('keeps the mounted conversation target when application preferences change', async () => {
        const { store } = setup();
        const hook = renderHook(
            ({ enabled }) => {
                useAppScreenshotShortcut(enabled);
                return useChatScreenshot(store, 'private:12', false);
            },
            { initialProps: { enabled: true } },
        );
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    enabled: true,
                    context: expect.stringContaining('/private:12/'),
                }),
            ),
        );
        const context = screenshot.shortcut.mock.calls.at(-1)![0].context;
        act(() => hook.result.current.updatePreferences({ hideWindow: false }));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true, context, hideWindow: false }),
            ),
        );
        act(() => hook.result.current.updatePreferences({ hideWindow: true }));
        hook.unmount();
    });

    it('re-registers the application shortcut after the feature is re-enabled', async () => {
        const hook = renderHook(({ enabled }) => useAppScreenshotShortcut(enabled), {
            initialProps: { enabled: false },
        });
        hook.rerender({ enabled: true });
        await waitFor(() => expect(screenshot.shortcut).toHaveBeenCalled());
        screenshot.shortcut.mockClear();
        hook.rerender({ enabled: false });
        hook.rerender({ enabled: true });
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true, context: '' }),
            ),
        );
        hook.unmount();
    });

    it('unregisters the global key while recording and enables it again afterwards', async () => {
        const { store } = setup();
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        act(() => hook.result.current.suspendShortcut(true));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: false, releaseOwner: false }),
            ),
        );
        act(() => hook.result.current.suspendShortcut(false));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        hook.unmount();
    });
    it('receives a native hotkey result without asking the hidden page to start capture', async () => {
        const { store } = setup();
        const receive = (event: ChatScreenshotShortcutEvent) => screenshot.handler?.(event);
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenCalledWith(
                expect.objectContaining({
                    enabled: true,
                    context: expect.any(String),
                    hideWindow: true,
                }),
            ),
        );
        const { context } = screenshot.shortcut.mock.calls.at(-1)![0];
        act(() =>
            receive({ v: 1, context, capture_id: 'native-hidden', result: { kind: 'started' } }),
        );
        expect(hook.result.current.capturing).toBe(true);
        expect(screenshot.capture).not.toHaveBeenCalled();
        act(() =>
            receive({
                v: 1,
                context,
                capture_id: 'native-hidden',
                result: { kind: 'finished', file },
            }),
        );
        expect(hook.result.current.capturing).toBe(false);
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(1);
        hook.unmount();
    });

    it('does not block capture when a full draft is not receiving the screenshot', async () => {
        const { store } = setup();
        const attachments = Array.from({ length: 8 }, (_, index) => ({
            key: String(index),
            type: 'face' as const,
            id: '14',
            name: '微笑',
        }));
        store.draft('private:12', { ...EMPTY_DRAFT, attachments });
        await captureChatScreenshot(store, 'private:12', { ...options, addToChat: false });
        expect(screenshot.capture).toHaveBeenCalledWith({ hideWindow: true, addToChat: false });
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(8);
    });

    it('respects the add-to-chat preference for native hotkey captures', async () => {
        const { store } = setup();
        const receive = (event: ChatScreenshotShortcutEvent) => screenshot.handler?.(event);
        const onFinished = vi.fn();
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false, onFinished));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        act(() => hook.result.current.updatePreferences({ addToChat: false }));
        await waitFor(() => {
            expect(hook.result.current.preferences.addToChat).toBe(false);
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ addToChat: false }),
            );
        });
        const { context } = screenshot.shortcut.mock.calls.at(-1)![0];
        act(() =>
            receive({ v: 1, context, capture_id: 'native-no-attach', result: { kind: 'started' } }),
        );
        act(() =>
            receive({
                v: 1,
                context,
                capture_id: 'native-no-attach',
                result: { kind: 'finished', file },
            }),
        );
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        expect(onFinished).not.toHaveBeenCalled();
        act(() => hook.result.current.updatePreferences({ addToChat: true }));
        hook.unmount();
    });

    it('does not refocus the composer after a clipboard-only toolbar capture', async () => {
        const { store } = setup();
        const onFinished = vi.fn();
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false, onFinished));
        act(() => hook.result.current.updatePreferences({ addToChat: false }));
        await act(async () => {
            await hook.result.current.start();
        });
        expect(screenshot.capture).toHaveBeenCalledWith({ hideWindow: true, addToChat: false });
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        expect(onFinished).not.toHaveBeenCalled();
        act(() => hook.result.current.updatePreferences({ addToChat: true }));
        hook.unmount();
    });

    it('routes a result to the old conversation after listener replacement and ignores duplicates', async () => {
        const { store } = setup();
        const receive = (event: ChatScreenshotShortcutEvent) => screenshot.handler?.(event);
        const hook = renderHook(({ session }) => useChatScreenshot(store, session, false), {
            initialProps: { session: 'private:12' as SessionKey },
        });
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        const { context } = screenshot.shortcut.mock.calls.at(-1)![0];
        store.getSnapshot().account.active = 'group:20';
        hook.rerender({ session: 'group:20' });
        await waitFor(() =>
            expect(screenshot.shortcut.mock.calls.at(-1)![0].context).not.toBe(context),
        );
        // started 在隐藏页或监听交接时迟到，finished 仍有注册时的原会话身份。
        const event: ChatScreenshotShortcutEvent = {
            v: 1,
            context,
            capture_id: 'native-replaced',
            result: { kind: 'finished', file },
        };
        act(() => {
            receive(event);
            receive(event);
        });
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(1);
        expect(store.getSnapshot().account.drafts['group:20']?.attachments ?? []).toHaveLength(0);
        expect(screenshot.capture).not.toHaveBeenCalled();
        hook.unmount();
    });

    it('rejects a native result after the originating view has been handed off', async () => {
        const { store } = setup();
        const receive = (event: ChatScreenshotShortcutEvent) => screenshot.handler?.(event);
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        const { context } = screenshot.shortcut.mock.calls.at(-1)![0];
        await cancelChatScreenshot();
        expect(screenshot.cancel).toHaveBeenCalledOnce();
        store.viewRevision++;
        act(() =>
            receive({
                v: 1,
                context,
                capture_id: 'native-handoff',
                result: { kind: 'finished', file },
            }),
        );
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        hook.unmount();
    });

    it('receives completion while the composer is unmounted and releases the capture lock', async () => {
        const { store } = setup();
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenCalledWith(
                expect.objectContaining({ enabled: true }),
            ),
        );
        const { context } = screenshot.shortcut.mock.calls.at(-1)![0];
        act(() =>
            screenshot.handler?.({
                v: 1,
                context,
                capture_id: 'native-unmounted',
                result: { kind: 'started' },
            }),
        );
        expect(hook.result.current.capturing).toBe(true);
        hook.unmount();
        store.releaseWhenIdle = true;
        act(() =>
            screenshot.handler?.({
                v: 1,
                context,
                capture_id: 'native-unmounted',
                result: { kind: 'finished', file },
            }),
        );
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        const next = setup().store;
        const remounted = renderHook(() => useChatScreenshot(next, 'private:12', false));
        expect(remounted.result.current.capturing).toBe(false);
        remounted.unmount();
    });

    it('keeps the image in the draft when automatic clipboard publication fails', async () => {
        const { store } = setup();
        screenshot.capture.mockResolvedValueOnce({ ...file, clipboardError: '剪贴板正在被占用' });
        await captureChatScreenshot(store, 'private:12', options);
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(1);
        expect(screenshot.notice).toHaveBeenLastCalledWith(
            expect.objectContaining({
                title: '截图已完成，剪贴板未更新',
                tone: 'warning',
                content: '剪贴板正在被占用',
            }),
        );
    });

    it('reconfigures native registration when switching global and focused scope', async () => {
        const { store } = setup();
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        act(() => hook.result.current.updatePreferences({ globalShortcut: false }));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true, global: false }),
            ),
        );
        act(() => hook.result.current.updatePreferences({ globalShortcut: true }));
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: true, global: true }),
            ),
        );
        hook.unmount();
    });
    it('returns to the originating conversation and merges the latest draft', async () => {
        const { store } = setup();
        const complete = pendingCapture();
        const pending = captureChatScreenshot(store, 'private:12', options);
        store.getSnapshot().account.active = 'group:20';
        const existing: Draft = {
            text: '截图期间继续编辑',
            reply: { id: '7', name: '好友', preview: '引用' },
            mentions: [],
            attachments: [{ key: 'existing', type: 'face', id: '14', name: '微笑' }],
        };
        store.draft('private:12', existing);
        store.draft('group:20', { ...EMPTY_DRAFT, text: '另一个群的草稿' });
        complete();
        await pending;
        expect(store.getSnapshot().account.drafts['private:12']).toEqual({
            ...existing,
            attachments: [
                ...existing.attachments,
                expect.objectContaining({
                    type: 'image',
                    path: file.path,
                    previewPath: file.previewPath,
                }),
            ],
        });
        expect(store.getSnapshot().account.drafts['group:20'].text).toBe('另一个群的草稿');
    });

    it('allows only one native capture across accounts', async () => {
        const { store } = setup();
        const other = setup().store;
        const complete = pendingCapture();
        const pending = captureChatScreenshot(store, 'private:12', options);
        await captureChatScreenshot(other, 'private:12', options);
        expect(screenshot.capture).toHaveBeenCalledOnce();
        complete(null);
        await pending;
        expect(other.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
    });

    it('rejects a full draft before capture and a draft filled while capture is pending', async () => {
        const { store } = setup();
        const attachments = Array.from({ length: 8 }, (_, index) => ({
            key: String(index),
            type: 'face' as const,
            id: '14',
            name: '微笑',
        }));
        store.draft('private:12', { ...EMPTY_DRAFT, attachments });
        await captureChatScreenshot(store, 'private:12', options);
        expect(screenshot.capture).not.toHaveBeenCalled();
        store.draft('private:12', { ...EMPTY_DRAFT });
        const complete = pendingCapture();
        const pending = captureChatScreenshot(store, 'private:12', options);
        store.draft('private:12', { ...EMPTY_DRAFT, attachments });
        complete();
        await pending;
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(8);
        expect(screenshot.notice).toHaveBeenLastCalledWith(
            expect.objectContaining({
                content: '一次最多添加 8 个附件',
            }),
        );
    });

    it('ignores late completion after cancellation and does not cancel another account', async () => {
        const { store } = setup();
        const complete = pendingCapture();
        const pending = captureChatScreenshot(store, 'private:12', options);
        await cancelChatScreenshot(setup().store);
        expect(screenshot.cancel).not.toHaveBeenCalled();
        await cancelChatScreenshot(store);
        expect(screenshot.cancel).toHaveBeenCalledOnce();
        complete();
        await pending;
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        expect(screenshot.notice).not.toHaveBeenCalled();
    });

    it('cancels native capture before a window handoff saves the view', async () => {
        const { store } = setup();
        const complete = pendingCapture();
        const pending = captureChatScreenshot(store, 'private:12', options);
        await prepareChatHandoff(null);
        expect(screenshot.cancel).toHaveBeenCalledOnce();
        complete();
        await pending;
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
    });

    it.each(['revision', 'release', 'identity'] as const)(
        'ignores an invalidated %s owner',
        async (reason) => {
            const { store } = setup();
            const complete = pendingCapture();
            const pending = captureChatScreenshot(store, 'private:12', options);
            if (reason === 'revision') store.viewRevision++;
            if (reason === 'release') store.releaseWhenIdle = true;
            if (reason === 'identity') store.getSnapshot().account.selfId = '100';
            complete();
            await pending;
            expect(store.getSnapshot().account.drafts['private:12'].attachments).toHaveLength(0);
        },
    );

    it('reads only the thumbnail and sends the original file through the attachment flow', async () => {
        const { store, transport } = setup();
        await store.connect();
        await captureChatScreenshot(store, 'private:12', options);
        const read = vi.spyOn(chatService, 'readLocalImage').mockResolvedValue('aGVsbG8=');
        const client = new QueryClient();
        const view = render(
            createElement(
                QueryClientProvider,
                { client },
                createElement(ChatComposer, {
                    store,
                    contact: { key: 'private:12', id: '12', name: '好友', type: 'private' },
                    disabledReason: '',
                }),
            ),
        );
        await screen.findByRole('img', { name: '截图.png' });
        expect(read).toHaveBeenCalledWith(file.previewPath);
        expect(read).not.toHaveBeenCalledWith(file.path);
        transport.call.mockClear();
        await act(async () => {
            await store.send('private:12');
        });
        await waitFor(() => expect(transport.call).toHaveBeenCalled());
        const params = transport.call.mock.calls[0][2];
        expect(params).toMatchObject({
            message: expect.arrayContaining([
                {
                    type: 'image',
                    data: { file: `ncd-local-file://${file.path}`, name: '截图.png', sub_type: 0 },
                },
            ]),
        });
        expect(JSON.stringify(params)).not.toContain(file.previewPath);
        view.unmount();
        await store.disconnect();
    });

    it('reports rejected native registration, releases the view, and retries a later mount', async () => {
        const { store } = setup();
        let nativeBinding: string | null = null;
        screenshot.shortcut.mockImplementation(
            async (request: { enabled: boolean; key: string; releaseOwner: boolean }) => {
                if (request.enabled && request.key === '2') throw new Error('control 参数无法识别');
                if (request.enabled) nativeBinding = request.key;
                else if (!request.releaseOwner) nativeBinding = null;
            },
        );
        const hook = renderHook(() => useChatScreenshot(store, 'private:12', false));
        act(() => hook.result.current.updatePreferences({ shortcut: 'Ctrl+Alt+1' }));
        await waitFor(() => expect(nativeBinding).toBe('1'));
        act(() => hook.result.current.updatePreferences({ shortcut: 'Ctrl+Alt+2' }));
        await waitFor(() =>
            expect(screenshot.notice).toHaveBeenLastCalledWith({
                key: 'chat:screenshot:shortcut',
                tone: 'warning',
                title: '截图快捷键未启用',
                content: 'control 参数无法识别',
            }),
        );
        expect(nativeBinding).toBe('1');
        const rejectedCalls = () =>
            screenshot.shortcut.mock.calls.filter(
                ([request]) => request.enabled && request.key === '2',
            ).length;
        expect(rejectedCalls()).toBe(1);
        hook.unmount();
        await waitFor(() =>
            expect(screenshot.shortcut).toHaveBeenLastCalledWith(
                expect.objectContaining({ enabled: false, releaseOwner: true }),
            ),
        );
        expect(nativeBinding).toBe('1');
        const retry = renderHook(() => useChatScreenshot(store, 'private:12', false));
        await waitFor(() => expect(rejectedCalls()).toBe(2));
        retry.unmount();
    });
});
