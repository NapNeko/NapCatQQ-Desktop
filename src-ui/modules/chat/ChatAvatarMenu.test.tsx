import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Contact, Message } from '../../core/domain/chat/model';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { ChatAvatarMenu } from './ChatAvatarMenu';
import { ChatMessageActions } from './ChatMessageActions';

const contact: Contact = { key: 'group:12', id: '12', name: '群', type: 'group' };
const baseMessage: Message = { key: 'group:12/2', session: 'group:12', id: '2', senderId: '20', senderName: '小明', at: 0, mine: false, segments: [{ type: 'text', data: { text: '内容' } }], status: 'sent' };

function setup(message = baseMessage, session: Contact = contact, connected = true) {
    const call = vi.fn(async () => ({ request_id: 'r', result: { kind: 'ok', outcome: { ok: true, retcode: 0, data: null } } }));
    const transport = { call, subscribe: vi.fn(), unsubscribe: vi.fn() };
    const store = new ChatAccountStore(
        { bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true },
        transport as never,
    );
    if (connected) store.getSnapshot().connection = { state: 'connected' };
    const onFocusComposer = vi.fn(); const onError = vi.fn();
    render(
        <ChatMessageActions store={store} contact={session} message={message} onFocusComposer={onFocusComposer} onError={onError}>
            {controls => <article>
                <ChatAvatarMenu store={store} contact={session} message={message} onFocusComposer={onFocusComposer} onError={onError}>
                    <img alt="头像" />
                </ChatAvatarMenu>
                <p>消息内容</p>{controls}
            </article>}
        </ChatMessageActions>);
    return { store, call, onFocusComposer, onError };
}

describe('ChatAvatarMenu', () => {
    it('opens only mention and poke from the avatar, never the message menu', async () => {
        const user = userEvent.setup();
        setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getAllByRole('menu')).toHaveLength(1);
        expect(screen.getByRole('menuitem', { name: '提及 小明' })).toBeInTheDocument();
        expect(screen.getByRole('menuitem', { name: '拍一拍 小明' })).toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: '回复' })).not.toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: '复制消息' })).not.toBeInTheDocument();
    });
    it('mentions the sender into the draft and focuses the composer', async () => {
        const user = userEvent.setup();
        const { store, onFocusComposer } = setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        await user.click(screen.getByRole('menuitem', { name: '提及 小明' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            text: '@小明 ', mentions: [{ qq: '20', label: '@小明' }], attachments: [], reply: null,
        });
        expect(onFocusComposer).toHaveBeenCalled();
    });
    it('pokes the sender in a group through group_poke and shows the hint optimistically', async () => {
        const user = userEvent.setup();
        const { store, call } = setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        await user.click(screen.getByRole('menuitem', { name: '拍一拍 小明' }));
        await waitFor(() => expect(call).toHaveBeenCalledWith('bot', 'group_poke', { group_id: '12', user_id: '20' }));
        expect(store.getSnapshot().account.messages.some(m => m.notice === '你拍了拍20')).toBe(true);
    });
    it('offers poke without mention in private chats and reports failures', async () => {
        const user = userEvent.setup();
        const session: Contact = { key: 'private:20', id: '20', name: '小明', type: 'private' };
        const { call, onError } = setup({ ...baseMessage, session: session.key }, session);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.queryByRole('menuitem', { name: /提及/ })).not.toBeInTheDocument();
        call.mockRejectedValueOnce(new Error('backend down'));
        await user.click(screen.getByRole('menuitem', { name: '拍一拍 小明' }));
        await waitFor(() => expect(call).toHaveBeenCalledWith('bot', 'friend_poke', { user_id: '20' }));
        await waitFor(() => expect(onError).toHaveBeenCalledWith('backend down'));
    });
    it('keeps the plain message menu on my own avatar', async () => {
        const user = userEvent.setup();
        setup({ ...baseMessage, mine: true, senderId: '99', senderName: '测试' });
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getByRole('menuitem', { name: '回复' })).toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: /拍一拍/ })).not.toBeInTheDocument();
    });
    it('disables poke while disconnected', async () => {
        const user = userEvent.setup();
        setup(baseMessage, contact, false);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByAltText('头像') });
        expect(screen.getByRole('menuitem', { name: '拍一拍 小明' })).toHaveAttribute('aria-disabled', 'true');
    });
});
