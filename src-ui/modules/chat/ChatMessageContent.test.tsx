import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Message } from '../../core/domain/chat/model';
import { ChatMessageContent } from './ChatMessageContent';

afterEach(cleanup);
const message: Message = {
    key: 'group:12/7',
    session: 'group:12',
    id: '7',
    senderId: '22',
    senderName: '小林',
    at: 1,
    mine: false,
    status: 'sent',
    recalled: true,
    segments: [{ type: 'text', data: { text: '收到的原文' } }],
};

describe('recalled message content', () => {
    it.each(['text', 'image', 'video', 'record', 'forward'])(
        '%s uses one consistent recall placeholder',
        (type) => {
            const { container } = render(
                <ChatMessageContent message={{ ...message, segments: [{ type, data: {} }] }} />,
            );
            expect(screen.getAllByText('消息已撤回')).toHaveLength(1);
            expect(container.querySelector('.native-chat-bubble')).toHaveClass(
                'is-recall-placeholder',
            );
            expect(container.querySelector('.native-chat-bubble')).not.toHaveClass('is-media-only');
        },
    );
    it('keeps text with an accessible recall description while protection is enabled', () => {
        const { rerender } = render(<ChatMessageContent message={message} preventRecall />);
        expect(screen.getByText('收到的原文')).toBeInTheDocument();
        expect(screen.getByRole('group')).toHaveAccessibleDescription('消息已撤回，内容已保留');
        expect(screen.getByRole('group')).toHaveClass('is-recall-retained');
        rerender(<ChatMessageContent message={message} preventRecall={false} />);
        expect(screen.queryByText('收到的原文')).not.toBeInTheDocument();
        expect(screen.getAllByText('消息已撤回')).toHaveLength(1);
    });
    it('preserves media inside the marked bubble and handles content that was never received', () => {
        const { rerender } = render(
            <ChatMessageContent
                message={{
                    ...message,
                    segments: [
                        { type: 'image', data: { url: 'https://example.test/sticker.gif' } },
                    ],
                }}
                preventRecall
            />,
        );
        expect(screen.getByRole('img', { name: '图片' })).toBeInTheDocument();
        expect(screen.getByRole('group')).toHaveClass('is-recall-retained');
        rerender(<ChatMessageContent message={{ ...message, segments: [] }} preventRecall />);
        expect(screen.getAllByText('消息已撤回')).toHaveLength(1);
        expect(screen.queryByRole('group')).not.toBeInTheDocument();
    });
});
