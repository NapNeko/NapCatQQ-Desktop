import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { Conversation } from '../../core/domain/chat/model';
import { ChatDetails } from './ChatDetails';

const friend: Conversation = { key: 'private:10022', id: '10022', type: 'private', name: '阿澄', unread: 0, pinned: false, lastAt: 0, preview: '' };

describe('native conversation details', () => {
    it('opens through the conversation title as well as its icon trigger', async () => {
        function Header() {
            const [open, setOpen] = useState(false);
            return <><button onClick={() => setOpen(true)}>查看阿澄的资料</button><ChatDetails contact={friend} onPin={() => {}} open={open} onOpenChange={setOpen} /></>;
        }
        render(<Header />);
        fireEvent.click(screen.getByRole('button', { name: '查看阿澄的资料' }));
        expect(await screen.findByRole('dialog', { name: '会话资料' })).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '关闭会话资料' }));
        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        expect(await screen.findByRole('dialog', { name: '会话资料' })).toBeInTheDocument();
    });
    it('opens private details by pointer and closes back to the trigger', async () => {
        const pin = vi.fn();
        render(<ChatDetails contact={friend} onPin={pin} />);
        const trigger = screen.getByRole('button', { name: '会话资料' });
        fireEvent.click(trigger);
        expect(await screen.findByRole('dialog', { name: '会话资料' })).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '置顶会话' }));
        expect(pin).toHaveBeenCalledOnce();
        fireEvent.click(screen.getByRole('button', { name: '关闭会话资料' }));
        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        await waitFor(() => expect(trigger).toHaveFocus());
    });

    it('lets group details move in and out of the message box', async () => {
        const box = vi.fn();
        const group: Conversation = { ...friend, key: 'group:20001', type: 'group', id: '20001', name: '开发交流' };
        const { rerender } = render(<ChatDetails contact={group} onPin={() => {}} onBox={box} boxed={false} />);
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        fireEvent.click(await screen.findByRole('button', { name: '移入群消息盒子' }));
        expect(box).toHaveBeenCalledOnce();
        rerender(<ChatDetails contact={group} onPin={() => {}} onBox={box} boxed />);
        fireEvent.click(screen.getByRole('button', { name: '移出群消息盒子' }));
        expect(box).toHaveBeenCalledTimes(2);
    });
});
