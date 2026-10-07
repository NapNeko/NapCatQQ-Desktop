import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Conversation } from '../../core/domain/chat/model';
import { ChatDetails } from './ChatDetails';
import { chatProfileService } from '../../core/services/chat-profile.service';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';

const friend: Conversation = {
    key: 'private:10022',
    id: '10022',
    type: 'private',
    name: '阿澄',
    unread: 0,
    pinned: false,
    lastAt: 0,
    preview: '',
};

describe('native conversation details', () => {
    beforeEach(() => {
        vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(240);
        vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(300);
    });
    it('opens through the conversation title as well as its icon trigger', async () => {
        function Header() {
            const [open, setOpen] = useState(false);
            return (
                <>
                    <button onClick={() => setOpen(true)}>查看阿澄的资料</button>
                    <ChatDetails
                        contact={friend}
                        onPin={() => {}}
                        open={open}
                        onOpenChange={setOpen}
                    />
                </>
            );
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

    it('keeps group information compact and opens members only from its shortcut', async () => {
        const target: DebugTarget = {
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        };
        vi.spyOn(chatProfileService, 'info').mockResolvedValue({
            name: '开发交流',
            fields: [{ label: '成员', value: '18 / 500 人' }],
        });
        const members = vi.spyOn(chatProfileService, 'members');
        const openMembers = vi.fn();
        const group: Conversation = {
            ...friend,
            key: 'group:20001',
            type: 'group',
            id: '20001',
            name: '开发交流',
        };
        render(
            <ChatDetails
                contact={group}
                target={target}
                onPin={() => {}}
                onMembers={openMembers}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        expect(await screen.findByText('18 / 500 人')).toBeInTheDocument();
        expect(screen.queryByRole('textbox', { name: '搜索群成员' })).not.toBeInTheDocument();
        expect(members).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '查看群成员' }));
        expect(openMembers).toHaveBeenCalledOnce();
    });
});
