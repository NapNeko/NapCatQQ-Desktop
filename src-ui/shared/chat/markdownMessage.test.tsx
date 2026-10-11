import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatForwardError } from '../../core/domain/chat/messageTransfer';
import type { Segment } from '../../core/domain/debug/segments';
import { ChatViewContext, useChatView, type ChatViewApi } from './chatContext';
import { SegmentList } from './SegmentView';

function Message({
    api,
    segments,
    messageId,
}: {
    api: Partial<ChatViewApi>;
    segments: Segment[];
    messageId: string;
}) {
    return (
        <ChatViewContext.Provider value={{ ...useChatView(), selfId: () => 99, ...api }}>
            <SegmentList segments={segments} mine={false} messageId={messageId} />
        </ChatViewContext.Provider>
    );
}

const command = (extra: Record<string, unknown> = {}): Segment => ({
    type: 'inline_keyboard',
    data: {
        bot_appid: '12345678',
        rows: [
            {
                buttons: [
                    {
                        id: 'status',
                        label: '查看状态',
                        visited_label: '已填入',
                        type: 2,
                        data: '/status',
                        permission_type: 2,
                        enter: false,
                        ...extra,
                    },
                ],
            },
        ],
    },
});

describe('QQ markdown message buttons', () => {
    it('renders protocol content and real links while gating callbacks with a reason', () => {
        const openLink = vi.fn();
        const onMarkdownCommand = vi.fn();
        render(
            <Message
                messageId="markdown-links"
                api={{ openLink, onMarkdownCommand }}
                segments={[
                    {
                        type: 'markdown',
                        data: { content: '[](%7B%22version%22%3A2%7D)\n# 标题\n正文' },
                    },
                    {
                        type: 'inline_keyboard',
                        data: {
                            rows: [
                                {
                                    buttons: [
                                        {
                                            label: '查看文档',
                                            type: 0,
                                            data: 'https://example.com/docs',
                                            permission_type: 2,
                                        },
                                        {
                                            label: '后台回调',
                                            type: 1,
                                            data: 'callback',
                                            permission_type: 2,
                                        },
                                    ],
                                },
                            ],
                        },
                    },
                ]}
            />,
        );
        expect(screen.getByText('标题')).toBeInTheDocument();
        expect(screen.queryByText(/version%22|inline_keyboard/)).not.toBeInTheDocument();
        expect(onMarkdownCommand).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '查看文档' }));
        expect(openLink).toHaveBeenCalledWith('https://example.com/docs');
        const callback = screen.getByRole('button', { name: '后台回调' });
        expect(callback).toBeDisabled();
        expect(callback).toHaveAttribute('title', '当前连接未提供 QQ 回调按钮能力');
        fireEvent.click(callback);
        expect(onMarkdownCommand).not.toHaveBeenCalled();
    });

    it('executes a command only on a click and preserves draft, reply and app-id flags', async () => {
        const onMarkdownCommand = vi.fn().mockResolvedValue(undefined);
        render(
            <Message
                messageId="markdown-draft"
                api={{ onMarkdownCommand, mediaScope: 'draft-test' }}
                segments={[command()]}
            />,
        );
        expect(onMarkdownCommand).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '查看状态' }));
        expect(onMarkdownCommand).toHaveBeenCalledWith({
            text: '/status',
            enter: false,
            reply: false,
            messageId: 'markdown-draft',
            botAppid: '12345678',
        });
        await screen.findByRole('button', { name: '已填入' });
    });

    it('locks pending and uncertain actions across a virtual row remount', async () => {
        let reject!: (error: Error) => void;
        const onMarkdownCommand = vi.fn(
            () =>
                new Promise<void>((_resolve, fail) => {
                    reject = fail;
                }),
        );
        const props = {
            messageId: 'markdown-uncertain',
            api: { onMarkdownCommand, mediaScope: 'uncertain-test' },
            segments: [command({ enter: true, is_reply: true })],
        };
        const first = render(<Message {...props} />);
        fireEvent.click(screen.getByRole('button', { name: '查看状态' }));
        expect(onMarkdownCommand).toHaveBeenCalledWith(
            expect.objectContaining({ enter: true, reply: true }),
        );
        first.unmount();
        const second = render(<Message {...props} />);
        expect(screen.getByRole('button', { name: '查看状态' })).toBeDisabled();
        await act(async () => {
            reject(new ChatForwardError('结果未确认', true));
        });
        await waitFor(() =>
            expect(screen.getByRole('button', { name: '查看状态' })).toHaveAttribute(
                'title',
                '结果待确认，请先检查会话',
            ),
        );
        second.unmount();
        render(<Message {...props} />);
        const locked = screen.getByRole('button', { name: '查看状态' });
        expect(locked).toBeDisabled();
        fireEvent.click(locked);
        expect(onMarkdownCommand).toHaveBeenCalledOnce();
    });

    it('keeps unknown permissions and unavailable command targets disabled', () => {
        render(
            <Message
                messageId="markdown-permissions"
                api={{}}
                segments={[command({ permission_type: 1 })]}
            />,
        );
        expect(screen.getByRole('button', { name: '查看状态' })).toBeDisabled();
        expect(screen.getByRole('button', { name: '查看状态' })).toHaveAttribute(
            'title',
            '此按钮需要 QQ 管理员权限',
        );
    });
});
