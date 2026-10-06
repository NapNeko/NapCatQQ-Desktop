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
    it('automatically exposes all member batches while keeping the DOM bounded and search global', async () => {
        const target: DebugTarget = {
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        };
        vi.spyOn(chatProfileService, 'info').mockResolvedValue({ name: '大群', fields: [] });
        const read = vi.spyOn(chatProfileService, 'members').mockResolvedValue(
            Array.from({ length: 150 }, (_, i) => ({
                key: `private:${i}` as const,
                type: 'private' as const,
                id: `${i}`,
                name: `成员${i}`,
                nickname: '',
                role: 'member',
                title: '',
            })),
        );
        render(
            <ChatDetails
                contact={{ ...friend, key: 'group:12', type: 'group', id: '12' }}
                target={target}
                onPin={() => {}}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        await screen.findByRole('button', { name: '查看成员0的群资料' });
        const list = screen.getByRole('region', { name: '群成员列表' });
        list.scrollTop = 48 * 55;
        fireEvent.scroll(list);
        await waitFor(() => expect(list.lastElementChild).toHaveStyle({ height: '5760px' }));
        list.scrollTop = 48 * 115;
        fireEvent.scroll(list);
        await waitFor(() => expect(list.lastElementChild).toHaveStyle({ height: '7200px' }));
        list.scrollTop = 48 * 145;
        fireEvent.scroll(list);
        expect(
            await screen.findByRole('button', { name: '查看成员149的群资料' }),
        ).toBeInTheDocument();
        expect(list.querySelectorAll('button').length).toBeLessThan(20);
        expect(screen.queryByRole('button', { name: '更多成员' })).not.toBeInTheDocument();
        fireEvent.change(screen.getByRole('textbox', { name: '搜索群成员' }), {
            target: { value: '成员149' },
        });
        expect(
            await screen.findByRole('button', { name: '查看成员149的群资料' }),
        ).toBeInTheDocument();
        expect(screen.getByRole('region', { name: '群成员列表' }).scrollTop).toBe(0);
        expect(read).toHaveBeenCalledOnce();
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

    it('loads group information, filters members and opens a private conversation', async () => {
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
        vi.spyOn(chatProfileService, 'members').mockResolvedValue([
            {
                key: 'private:10022',
                type: 'private',
                id: '10022',
                name: '群内名片',
                nickname: '阿澄',
                role: 'admin',
                title: '',
            },
        ]);
        const message = vi.fn();
        const group: Conversation = {
            ...friend,
            key: 'group:20001',
            type: 'group',
            id: '20001',
            name: '开发交流',
        };
        render(
            <ChatDetails contact={group} target={target} onPin={() => {}} onMessage={message} />,
        );
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        expect(await screen.findByText('18 / 500 人')).toBeInTheDocument();
        fireEvent.change(screen.getByRole('textbox', { name: '搜索群成员' }), {
            target: { value: '不存在' },
        });
        expect(screen.getByText('没有找到成员')).toBeInTheDocument();
        fireEvent.change(screen.getByRole('textbox', { name: '搜索群成员' }), {
            target: { value: '阿澄' },
        });
        fireEvent.click(screen.getByRole('button', { name: '查看群内名片的群资料' }));
        fireEvent.click(screen.getByRole('button', { name: '发消息' }));
        expect(message).toHaveBeenCalledWith(expect.objectContaining({ key: 'private:10022' }));
        expect(screen.queryByRole('button', { name: /移入|移出/ })).not.toBeInTheDocument();
    });
    it('keeps profile information visible when the member request fails and permits retry', async () => {
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
            fields: [{ label: '备注', value: '群备注' }],
        });
        const members = vi
            .spyOn(chatProfileService, 'members')
            .mockRejectedValueOnce(new Error('成员加载失败'))
            .mockResolvedValueOnce([]);
        render(
            <ChatDetails
                contact={{ ...friend, key: 'group:20001', type: 'group', id: '20001' }}
                target={target}
                onPin={() => {}}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '会话资料' }));
        expect(await screen.findByText('群备注')).toBeInTheDocument();
        fireEvent.click(await screen.findByRole('button', { name: '重新读取成员' }));
        await waitFor(() => expect(members).toHaveBeenCalledTimes(2));
        expect(screen.getByText('群备注')).toBeInTheDocument();
    });
});
