import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact, Message } from '../../core/domain/chat/model';
import { NativeTimeline } from './ChatTimeline';
import { chatMediaService } from '../../core/services/chat-media.service';

const contentRender = vi.hoisted(() => vi.fn());
vi.mock('./ChatMessageContent', async (importOriginal) => {
    const original = await importOriginal<typeof import('./ChatMessageContent')>();
    const { useChatView } = await import('../../shared/chat/chatContext');
    return {
        ChatMessageContent: (props: { message: Message; preventRecall?: boolean }) => {
            // 文字、图片和 @ 段都消费这份 context；记录一次可见消息内容的渲染。
            useChatView();
            contentRender();
            return <original.ChatMessageContent {...props} />;
        },
    };
});

vi.mock('./chatMotion', async (importOriginal) => ({
    ...(await importOriginal<typeof import('./chatMotion')>()),
    ChatMessageEntrance: ({
        animate,
        messageKey,
        children,
    }: {
        animate: boolean;
        messageKey: string;
        children: ReactNode;
    }) => (
        <div data-testid={`entry-${messageKey}`} data-animate={animate}>
            {children}
        </div>
    ),
}));

vi.mock('@tanstack/react-virtual', async (importOriginal) => ({
    ...(await importOriginal<typeof import('@tanstack/react-virtual')>()),
    useVirtualizer: (options: { count: number; getItemKey: (index: number) => string }) => ({
        options,
        containerRef: vi.fn(),
        getTotalSize: () => options.count * 68,
        getVirtualItems: () =>
            Array.from({ length: options.count }, (_, index) => ({
                index,
                start: index * 68,
                size: 68,
                key: options.getItemKey(index),
            })),
        measureElement: vi.fn(),
        scrollToIndex: vi.fn(),
    }),
}));

describe('timeline sending feedback', () => {
    it('skips initial history entrances while allowing the first live message in a loaded empty conversation', () => {
        const contact: Contact = { key: 'private:2', type: 'private', id: '2', name: '小红' };
        const store = new ChatAccountStore(
            {
                bot_id: 'timeline-entrance',
                name: '测试',
                qq_id: 99,
                backend: 'napcat',
                host: { kind: 'local' },
                running: true,
                online: true,
            },
            { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
        );
        vi.spyOn(store, 'ensureHistory').mockResolvedValue(undefined);
        store.getSnapshot().history[contact.key] = {
            loading: false,
            loaded: true,
            done: true,
            error: '',
        };
        const message: Message = {
            key: 'live',
            session: contact.key,
            senderId: '2',
            senderName: '小红',
            mine: false,
            at: Date.now(),
            status: 'sent',
            segments: [{ type: 'text', data: { text: '新消息' } }],
        };
        const client = new QueryClient();
        const timeline = (messages: Message[], key: string) => (
            <QueryClientProvider client={client}>
                <NativeTimeline
                    key={key}
                    store={store}
                    contact={contact}
                    messages={messages}
                    visible
                    revealRef={{ current: vi.fn() }}
                />
            </QueryClientProvider>
        );
        const { rerender } = render(timeline([], 'empty'));
        rerender(timeline([message], 'empty'));
        expect(screen.getByTestId('entry-live')).toHaveAttribute('data-animate', 'true');
        rerender(timeline([message], 'history'));
        expect(screen.getByTestId('entry-live')).toHaveAttribute('data-animate', 'false');
    });

    it('updates progress for a pending message without replacing the memoized message', async () => {
        const contact: Contact = { key: 'private:2', type: 'private', id: '2', name: '小红' };
        const store = new ChatAccountStore(
            {
                bot_id: 'timeline-progress',
                name: '测试',
                qq_id: 99,
                backend: 'napcat',
                host: { kind: 'local' },
                running: true,
                online: true,
            },
            { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
        );
        vi.spyOn(store, 'ensureHistory').mockResolvedValue(undefined);
        const message: Message = {
            key: 'pending-file',
            session: contact.key,
            senderId: '99',
            senderName: '测试',
            mine: true,
            at: Date.now(),
            status: 'sending',
            requestId: 'file-request',
            segments: [{ type: 'text', data: { text: '附件发送中' } }],
        };
        const messages = [message];
        const progress = {
            v: 1,
            request_id: message.requestId!,
            stage: 'reading' as const,
            file_name: '报告.pdf',
            done_bytes: 20,
            total_bytes: 100,
            done_chunks: 1,
            total_chunks: 5,
        };
        store.getSnapshot().sendProgress = { [progress.request_id]: progress };
        render(
            <QueryClientProvider client={new QueryClient()}>
                <NativeTimeline
                    store={store}
                    contact={contact}
                    messages={messages}
                    visible
                    revealRef={{ current: vi.fn() }}
                />
            </QueryClientProvider>,
        );
        expect(await screen.findByRole('progressbar')).toHaveAttribute('aria-valuenow', '20');
        act(() => {
            store.getSnapshot().sendProgress = {
                [progress.request_id]: { ...progress, stage: 'uploading', done_bytes: 80 },
            };
            store.draft(contact.key, { text: '', reply: null, attachments: [] });
        });
        expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '80');
        expect(screen.getByRole('status', { name: '上传 报告.pdf · 80%' })).toHaveAttribute(
            'title',
            '上传 报告.pdf · 80%',
        );
        expect(messages[0]).toBe(message);
    });
});

it('keeps visible message content stable for unrelated conversations but refreshes a referenced name', () => {
    const contact: Contact = { key: 'group:1', type: 'group', id: '1', name: '正在阅读的群' };
    const store = new ChatAccountStore(
        {
            bot_id: 'timeline-isolation',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        },
        { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
    );
    vi.spyOn(store, 'ensureHistory').mockResolvedValue(undefined);
    const messages: Message[] = Array.from({ length: 20 }, (_, index) => ({
        key: `visible-${index}`,
        id: String(index + 1),
        session: contact.key,
        senderId: '1',
        senderName: '群成员',
        mine: false,
        at: index * 1000,
        status: 'sent',
        segments:
            index === 0
                ? [{ type: 'at', data: { qq: '2' } }]
                : [{ type: 'text', data: { text: `消息 ${index}` } }],
    }));
    const reference: Message = {
        ...messages[1],
        key: 'other-name',
        session: 'private:2',
        senderId: '2',
        senderName: '原昵称',
    };
    store.getSnapshot().account.messages = [...messages, reference];
    const history = { loading: false, loaded: true, done: true, error: '' };
    store.getSnapshot().history = { [contact.key]: history };
    render(
        <QueryClientProvider client={new QueryClient()}>
            <NativeTimeline
                store={store}
                contact={contact}
                messages={messages}
                visible
                revealRef={{ current: vi.fn() }}
            />
        </QueryClientProvider>,
    );
    expect(screen.getByText('@原昵称')).toBeInTheDocument();
    contentRender.mockClear();
    act(() => {
        const snapshot = store.getSnapshot();
        snapshot.account = {
            ...snapshot.account,
            messages: [
                ...snapshot.account.messages,
                {
                    ...reference,
                    key: 'unrelated',
                    session: 'private:3',
                    senderId: '3',
                    senderName: '另一个人',
                },
            ],
        };
        snapshot.history = { ...snapshot.history, 'private:3': { ...history, loading: true } };
        snapshot.sendProgress = {
            unrelated: {
                v: 1,
                request_id: 'unrelated',
                stage: 'uploading',
                file_name: 'other.pdf',
                done_bytes: 1,
                total_bytes: 10,
                done_chunks: 1,
                total_chunks: 10,
            },
        };
        store.draft('private:3', { text: '草稿', attachments: [], reply: null });
    });
    expect(contentRender).not.toHaveBeenCalled();
    act(() => {
        const snapshot = store.getSnapshot();
        snapshot.account = {
            ...snapshot.account,
            messages: snapshot.account.messages.map((message) =>
                message === reference ? { ...message, senderName: '更新后的昵称' } : message,
            ),
        };
        store.draft('private:2', { text: '', attachments: [], reply: null });
    });
    expect(screen.getByText('@更新后的昵称')).toBeInTheDocument();
    act(() => {
        store.getSnapshot().history = {
            ...store.getSnapshot().history,
            [contact.key]: { ...history, loading: true },
        };
        store.draft(contact.key, { text: '', attachments: [], reply: null });
    });
    expect(screen.getByLabelText('消息记录')).toHaveAttribute('aria-busy', 'true');
});

it('keeps names inside an open forward responsive to other conversations', async () => {
    const contact: Contact = { key: 'private:1', type: 'private', id: '1', name: '好友' };
    const store = new ChatAccountStore(
        {
            bot_id: 'timeline-forward-names',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        },
        { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
    );
    vi.spyOn(store, 'ensureHistory').mockResolvedValue(undefined);
    vi.spyOn(chatMediaService, 'forward').mockResolvedValue([
        { senderId: '3', name: '转发者', segments: [{ type: 'at', data: { qq: '2' } }] },
    ]);
    const forward: Message = {
        key: 'forward-name',
        session: contact.key,
        senderId: '1',
        senderName: '好友',
        mine: false,
        at: 1000,
        status: 'sent',
        segments: [{ type: 'forward', data: { id: 'forward-name-test' } }],
    };
    const reference: Message = {
        ...forward,
        key: 'name-source',
        session: 'private:2',
        senderId: '2',
        senderName: '旧昵称',
        segments: [],
    };
    store.getSnapshot().account.messages = [forward, reference];
    render(
        <QueryClientProvider client={new QueryClient()}>
            <NativeTimeline
                store={store}
                contact={contact}
                messages={[forward]}
                visible
                revealRef={{ current: vi.fn() }}
            />
        </QueryClientProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
    expect(await screen.findByText('@旧昵称')).toBeInTheDocument();
    act(() => {
        const snapshot = store.getSnapshot();
        snapshot.account = {
            ...snapshot.account,
            messages: [forward, { ...reference, senderName: '新昵称' }],
        };
        store.draft('private:2', { text: '', reply: null, attachments: [] });
    });
    expect(screen.getByText('@新昵称')).toBeInTheDocument();
});
