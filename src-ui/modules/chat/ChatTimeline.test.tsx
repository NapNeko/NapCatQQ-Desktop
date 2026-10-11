import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact, Message } from '../../core/domain/chat/model';
import { NativeTimeline } from './ChatTimeline';

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
