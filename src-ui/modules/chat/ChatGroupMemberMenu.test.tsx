import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Contact, Message } from '../../core/domain/chat/model';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { chatProfileService } from '../../core/services/chat-profile.service';
import { ChatGroupMemberMenu } from './ChatGroupMemberMenu';
import { ChatMessageActions } from './ChatMessageActions';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';

describe('group avatar member menu', () => {
    beforeEach(() => groupMemberPermissions.clear());
    it('preserves avatar actions without nesting message menus and exposes management only after role checks', async () => {
        const user = userEvent.setup();
        const now = Date.now();
        const clock = vi.spyOn(Date, 'now').mockReturnValue(now);
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
            { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
        );
        store.getSnapshot().connection = { state: 'connected' };
        const contact: Contact = { key: 'group:12', id: '12', type: 'group', name: '群' };
        const message: Message = {
            key: 'group:12/2',
            session: 'group:12',
            id: '2',
            senderId: '20',
            senderName: '小明',
            at: 1,
            mine: false,
            segments: [{ type: 'text', data: { text: '正文' } }],
            status: 'sent',
        };
        let resolveSelf!: (value: Awaited<ReturnType<typeof chatProfileService.member>>) => void;
        vi.spyOn(chatProfileService, 'member').mockImplementation(
            async (_target, _group, userId) =>
                userId === '99'
                    ? new Promise<Awaited<ReturnType<typeof chatProfileService.member>>>(
                          (resolve) => {
                              resolveSelf = resolve;
                          },
                      )
                    : {
                          key: 'private:20',
                          type: 'private',
                          id: '20',
                          name: '小明',
                          nickname: '',
                          title: '',
                          role: 'member',
                      },
        );
        const onViewMember = vi.fn();
        render(
            <ChatMessageActions store={store} contact={contact} message={message} onError={vi.fn()}>
                {(controls) => (
                    <article>
                        <ChatGroupMemberMenu
                            target={store.target}
                            store={store}
                            contact={contact}
                            message={message}
                            onMessage={vi.fn()}
                            onViewMember={onViewMember}
                        >
                            <img alt="头像" />
                        </ChatGroupMemberMenu>
                        <p>正文</p>
                        {controls}
                    </article>
                )}
            </ChatMessageActions>,
        );
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getAllByRole('menu')).toHaveLength(1);
        expect(screen.getByRole('menuitem', { name: '提及 小明' })).toBeInTheDocument();
        expect(screen.getByRole('menuitem', { name: '拍一拍 小明' })).toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: '回复' })).not.toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: '禁言' })).not.toBeInTheDocument();
        await act(async () =>
            resolveSelf({
                key: 'private:99',
                type: 'private',
                id: '99',
                name: '我',
                nickname: '',
                title: '',
                role: 'owner',
            }),
        );
        expect(await screen.findByRole('menuitem', { name: '禁言' })).toBeInTheDocument();
        await user.click(screen.getByRole('menuitem', { name: '查看群名片' }));
        expect(onViewMember).toHaveBeenCalledWith('20');
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getByRole('menuitem', { name: '禁言' })).toBeInTheDocument();
        expect(vi.mocked(chatProfileService.member).mock.calls.map((args) => args[2])).toEqual([
            '99',
            '20',
        ]);
        await user.click(screen.getByRole('menuitem', { name: '查看群名片' }));
        clock.mockReturnValue(now + 30_000);
        let refresh!: typeof resolveSelf;
        vi.mocked(chatProfileService.member).mockImplementationOnce(
            () =>
                new Promise<Awaited<ReturnType<typeof chatProfileService.member>>>((resolve) => {
                    refresh = resolve;
                }),
        );
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getByRole('menuitem', { name: '禁言' })).toBeInTheDocument();
        expect(vi.mocked(chatProfileService.member).mock.calls.map((args) => args[2])).toEqual([
            '99',
            '20',
            '99',
        ]);
        await act(async () =>
            refresh({
                key: 'private:99',
                type: 'private',
                id: '99',
                name: '我',
                nickname: '',
                title: '',
                role: 'owner',
            }),
        );
    });
});
