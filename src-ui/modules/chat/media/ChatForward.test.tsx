import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SegmentList } from '../../debug/right/SegmentView';
import { ChatViewContext, useChatView } from '../../debug/right/chatContext';
import type { ForwardNode } from '../../../core/services/chat-media.service';

function Records({ read }: { read: (data: Record<string, unknown>) => Promise<ForwardNode[]> }) {
    return (
        <ChatViewContext.Provider value={{ ...useChatView(), readForward: read }}>
            <SegmentList mine={false} segments={[{ type: 'forward', data: { id: 'root' } }]} />
        </ChatViewContext.Provider>
    );
}
describe('forward record navigation', () => {
    it('opens an image above the records and closes it without closing or moving the records', async () => {
        const read = vi.fn().mockResolvedValue([
            {
                senderId: '12',
                name: '小明',
                segments: [
                    {
                        type: 'image',
                        data: {
                            url: 'https://example.test/forward-image.png',
                            width: 100,
                            height: 300,
                        },
                    },
                ],
            },
        ]);
        render(<Records read={read} />);
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        const thumbnail = await screen.findByAltText('图片');
        fireEvent.load(thumbnail);
        const body = screen.getByLabelText('转发消息记录');
        body.scrollTop = 80;
        fireEvent.click(thumbnail);
        const picture = await screen.findByAltText('消息图片');
        expect(picture.closest('.native-chat-image-dialog')?.closest('.fixed')).toHaveStyle({
            zIndex: 70,
        });
        fireEvent.keyDown(screen.getByLabelText('图片画布，滚轮缩放，拖动查看'), { key: 'Escape' });
        await waitFor(() => expect(screen.queryByAltText('消息图片')).not.toBeInTheDocument());
        expect(screen.getByLabelText('转发消息记录')).toBe(body);
        expect(body.scrollTop).toBe(80);
        expect(read).toHaveBeenCalledOnce();
    });
    it('navigates nested records in one dialog and restores the parent position without refetching', async () => {
        const read = vi
            .fn()
            .mockResolvedValueOnce([
                {
                    senderId: '12',
                    name: '小明',
                    segments: [
                        { type: 'text', data: { text: '父层' } },
                        { type: 'forward', data: { id: 'child' } },
                    ],
                },
            ])
            .mockResolvedValueOnce([
                {
                    senderId: '13',
                    name: '小李',
                    segments: [
                        { type: 'text', data: { text: '子层' } },
                        { type: 'forward', data: { id: 'root' } },
                    ],
                },
            ]);
        render(<Records read={read} />);
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        await screen.findByText('父层');
        screen.getByLabelText('转发消息记录').scrollTop = 160;
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        await screen.findByText('子层');
        expect(document.querySelectorAll('.native-chat-forward-dialog')).toHaveLength(1);
        expect(screen.getByRole('button', { name: '查看聊天记录' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '返回上一层聊天记录' }));
        expect(screen.getByText('父层')).toBeInTheDocument();
        expect(screen.getByLabelText('转发消息记录').scrollTop).toBe(160);
        expect(read).toHaveBeenCalledTimes(2);
    });
    it('ignores a nested response that arrives after returning to its parent', async () => {
        let finish!: (nodes: ForwardNode[]) => void;
        const read = vi
            .fn()
            .mockResolvedValueOnce([
                {
                    senderId: '12',
                    name: '小明',
                    segments: [
                        { type: 'text', data: { text: '父层' } },
                        { type: 'forward', data: { id: 'pending' } },
                    ],
                },
            ])
            .mockImplementationOnce(
                () =>
                    new Promise<ForwardNode[]>((resolve) => {
                        finish = resolve;
                    }),
            );
        render(<Records read={read} />);
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        await screen.findByText('父层');
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        fireEvent.click(await screen.findByRole('button', { name: '返回上一层聊天记录' }));
        await act(async () =>
            finish([
                {
                    senderId: '13',
                    name: '迟到',
                    segments: [{ type: 'text', data: { text: '迟到内容' } }],
                },
            ]),
        );
        expect(screen.getByText('父层')).toBeInTheDocument();
        expect(screen.queryByText('迟到内容')).not.toBeInTheDocument();
    });
    it('shows QQ-style preview lines from the first records and reuses them for the dialog', async () => {
        const read = vi.fn().mockResolvedValue([
            { senderId: '12', name: '小明', segments: [{ type: 'text', data: { text: '你好' } }] },
            {
                senderId: '13',
                name: '小李',
                segments: [{ type: 'image', data: { url: 'https://example.test/a.png' } }],
            },
            { senderId: '12', name: '小明', segments: [{ type: 'text', data: { text: '在吗' } }] },
            {
                senderId: '14',
                name: '小王',
                segments: [{ type: 'text', data: { text: '第四条不进预览' } }],
            },
        ]);
        render(<Records read={read} />);
        expect(await screen.findByText('小明: 你好')).toBeInTheDocument();
        expect(screen.getByText('小李: [图片]')).toBeInTheDocument();
        expect(screen.getByText('小明: 在吗')).toBeInTheDocument();
        expect(screen.queryByText(/第四条/)).not.toBeInTheDocument();
        expect(screen.getByText('查看转发记录')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        expect(await screen.findByText('第四条不进预览')).toBeInTheDocument();
        expect(read).toHaveBeenCalledOnce();
    });
});
