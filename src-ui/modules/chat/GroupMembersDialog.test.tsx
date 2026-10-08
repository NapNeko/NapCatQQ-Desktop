import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { chatProfileService, type ProfileMember } from '../../core/services/chat-profile.service';
import { GroupMembersDialog } from './GroupMembersDialog';
import { ChatMemberActions, canManageGroupMember } from './ChatMemberActions';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';

const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const group: Contact = { key: 'group:12', type: 'group', id: '12', name: '开发交流' };
const member: ProfileMember = {
    key: 'private:20',
    type: 'private',
    id: '20',
    name: '群内名片',
    nickname: '昵称',
    role: 'member',
    title: '',
};
const self: ProfileMember = { ...member, key: 'private:99', id: '99', name: '我', role: 'owner' };

const withQuery = (ui: ReactNode) => (
    <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
        {ui}
    </QueryClientProvider>
);

describe('group member panel', () => {
    beforeEach(() => {
        groupMemberPermissions.clear();
        vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(240);
        vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(300);
        vi.spyOn(chatProfileService, 'member').mockImplementation(
            async (_target, _group, userId) =>
                userId === '99' ? self : { ...member, id: userId, key: `private:${userId}` },
        );
    });
    it('exposes every batch with bounded DOM and searches the complete group', async () => {
        const read = vi.spyOn(chatProfileService, 'members').mockResolvedValue(
            Array.from({ length: 150 }, (_, i) => ({
                ...member,
                id: `${i + 1}`,
                key: `private:${i + 1}` as const,
                name: `成员${i}`,
            })),
        );
        render(
            withQuery(
                <GroupMembersDialog
                    open
                    target={target}
                    contact={group}
                    connected
                    onOpenChange={vi.fn()}
                    onMessage={vi.fn()}
                />,
            ),
        );
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
        fireEvent.change(screen.getByRole('textbox', { name: '搜索群成员' }), {
            target: { value: '成员149' },
        });
        expect(
            await screen.findByRole('button', { name: '查看成员149的群资料' }),
        ).toBeInTheDocument();
        expect(screen.getByRole('region', { name: '群成员列表' }).scrollTop).toBe(0);
        expect(read).toHaveBeenCalledOnce();
    });
    it('keeps management hidden when permission is unknown and opens a private chat from the member card', async () => {
        vi.spyOn(chatProfileService, 'members').mockResolvedValue([member]);
        vi.mocked(chatProfileService.member).mockImplementation(async (_target, _group, userId) =>
            userId === '99' ? { ...self, role: '' } : member,
        );
        const onMessage = vi.fn();
        render(
            withQuery(
                <GroupMembersDialog
                    open
                    target={target}
                    contact={group}
                    connected
                    onOpenChange={vi.fn()}
                    onMessage={onMessage}
                />,
            ),
        );
        fireEvent.click(await screen.findByRole('button', { name: '查看群内名片的群资料' }));
        await waitFor(() =>
            expect(chatProfileService.member).toHaveBeenCalledWith(target, '12', '20'),
        );
        expect(screen.queryByRole('button', { name: '禁言' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '移出群' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '发消息' }));
        expect(onMessage).toHaveBeenCalledWith(
            expect.objectContaining({ key: 'private:20', name: '群内名片' }),
        );
    });
    it('uses fresh target roles before exposing management', async () => {
        vi.spyOn(chatProfileService, 'members').mockResolvedValue([member]);
        vi.mocked(chatProfileService.member).mockImplementation(async (_target, _group, userId) =>
            userId === '99' ? { ...self, role: 'admin' } : { ...member, role: 'admin' },
        );
        render(
            withQuery(
                <GroupMembersDialog
                    open
                    target={target}
                    contact={group}
                    connected
                    onOpenChange={vi.fn()}
                    onMessage={vi.fn()}
                />,
            ),
        );
        fireEvent.click(await screen.findByRole('button', { name: '查看群内名片的群资料' }));
        await screen.findByText('管理员');
        expect(screen.queryByRole('button', { name: '禁言' })).not.toBeInTheDocument();
    });
});

describe('group member writes', () => {
    it('permits only lower roles and excludes self, owner and unknown roles', () => {
        expect(canManageGroupMember('99', 'owner', member)).toBe(true);
        expect(canManageGroupMember('99', 'owner', { ...member, role: 'admin' })).toBe(true);
        expect(canManageGroupMember('99', 'admin', member)).toBe(true);
        for (const [selfRole, other] of [
            ['admin', { ...member, role: 'admin' }],
            ['owner', { ...member, role: 'owner' }],
            ['owner', { ...member, role: '' }],
            ['', member],
            ['member', member],
            ['owner', { ...member, id: '99' }],
        ] as const)
            expect(canManageGroupMember('99', selfRole, other)).toBe(false);
    });
    it('confirms a removal and reports an uncertain result without retrying', async () => {
        const kick = vi
            .spyOn(chatProfileService, 'kick')
            .mockRejectedValue(new Error('请求超时，结果未知'));
        const onResult = vi.fn();
        const onClose = vi.fn();
        render(
            withQuery(
                <ChatMemberActions
                    target={target}
                    contact={group}
                    member={member}
                    action="kick"
                    connected
                    onClose={onClose}
                    onResult={onResult}
                />,
            ),
        );
        expect(screen.getByText(/对方仍可申请重新加入/)).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '移出群' }));
        await waitFor(() =>
            expect(onResult).toHaveBeenCalledWith({
                tone: 'danger',
                message: '请求超时，结果未知',
            }),
        );
        expect(kick).toHaveBeenCalledOnce();
        expect(kick).toHaveBeenCalledWith(target, '12', '20');
        expect(onClose).toHaveBeenCalledOnce();
    });
    it('ignores a late result after switching accounts', async () => {
        let finish!: () => void;
        const kick = vi.spyOn(chatProfileService, 'kick').mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    finish = resolve;
                }),
        );
        const onResult = vi.fn();
        const onClose = vi.fn();
        const view = render(
            withQuery(
                <ChatMemberActions
                    target={target}
                    contact={group}
                    member={member}
                    action="kick"
                    connected
                    onClose={onClose}
                    onResult={onResult}
                />,
            ),
        );
        fireEvent.click(screen.getByRole('button', { name: '移出群' }));
        // mutateAsync 要到微任务派发才真正调 mutationFn;先等 kick 起飞,finish 才拿到 resolve。
        await waitFor(() => expect(kick).toHaveBeenCalledOnce());
        view.rerender(
            withQuery(
                <ChatMemberActions
                    target={{ ...target, bot_id: 'another' }}
                    contact={group}
                    member={member}
                    action="kick"
                    connected
                    onClose={onClose}
                    onResult={onResult}
                />,
            ),
        );
        await act(async () => {
            finish();
            await new Promise((resolve) => setTimeout(resolve, 0));
        });
        expect(onResult).not.toHaveBeenCalled();
        expect(onClose).not.toHaveBeenCalled();
    });
});
