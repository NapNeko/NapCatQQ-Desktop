import { createRef } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Contact, Draft, Message } from '../../core/domain/chat/model';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { ChatMessageActions } from './ChatMessageActions';

const contact: Contact = { key: 'group:12', id: '12', name: '群', type: 'group' };
const baseMessage: Message = { key: 'group:12/2', session: 'group:12', id: '2', senderId: '20', senderName: '小明', at: 0, mine: false, segments: [{ type: 'text', data: { text: '  第一行\n第二行  ' } }], status: 'sent' };
const draft: Draft = { text: '正在写的文字', mentions: [], attachments: [{ key: 'a', type: 'file', name: '备注.txt', path: 'C:/备注.txt' }], reply: { id: '9', name: '小李', preview: '上一条' } };

function setup(message = baseMessage, session = contact) {
    const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
    store.draft(session.key, draft);
    const input = createRef<HTMLTextAreaElement>();
    const onFocusComposer = vi.fn(() => input.current?.focus());
    const onError = vi.fn();
    render(<>
        <ChatMessageActions store={store} contact={session} message={message} onFocusComposer={onFocusComposer} onError={onError}>
            {controls => <article><p>消息内容</p>{controls}</article>}
        </ChatMessageActions>
        <textarea ref={input} aria-label="输入消息" />
    </>);
    return { store, onFocusComposer, onError };
}

describe('ChatMessageActions', () => {
    it('replies from the context menu using the latest draft and keeps focus in the composer', async () => {
        const user = userEvent.setup();
        const { store } = setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        store.draft(contact.key, { ...draft, text: '右键菜单打开后又写的内容' });
        await user.click(screen.getByRole('menuitem', { name: '回复' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            ...draft, text: '右键菜单打开后又写的内容', reply: { id: '2', name: '小明', preview: '  第一行\n第二行  ' },
        });
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
        expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    });
    it('mentions from the menu without losing attachments or reply or adding a taller side toolbar', async () => {
        const user = userEvent.setup();
        const { store } = setup();
        expect(screen.getAllByRole('button')).toHaveLength(2);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '提及 小明' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({ ...draft, text: '正在写的文字 @小明 ', mentions: [{ qq: '20', label: '@小明' }] });
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '提及 小明' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({ ...draft, text: '正在写的文字 @小明 @小明 ', mentions: [{ qq: '20', label: '@小明' }] });
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
    });
    it.each(['Shift+F10', 'ContextMenu'])('opens message actions from a keyboard-focused control with %s', async shortcut => {
        const user = userEvent.setup();
        setup();
        screen.getByRole('button', { name: '复制消息' }).focus();
        await user.keyboard(shortcut === 'Shift+F10' ? '{Shift>}{F10}{/Shift}' : '{ContextMenu}');
        expect(await screen.findByRole('menuitem', { name: '提及 小明' })).toBeInTheDocument();
    });
    it('allows focusing the message itself to open actions with the keyboard', async () => {
        const user = userEvent.setup();
        setup();
        await user.tab();
        expect(screen.getByRole('article')).toHaveFocus();
        await user.keyboard('{Shift>}{F10}{/Shift}');
        expect(await screen.findByRole('menuitem', { name: '提及 小明' })).toBeInTheDocument();
    });
    it('copies exact message text and announces success', async () => {
        const user = userEvent.setup();
        const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue();
        setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '复制消息' }));
        expect(writeText).toHaveBeenCalledWith('  第一行\n第二行  ');
        expect(await screen.findByRole('button', { name: '消息已复制' })).toBeInTheDocument();
        expect(screen.getByRole('status')).toHaveTextContent('已复制消息');
    });
    it('reports a denied clipboard write without announcing success', async () => {
        userEvent.setup();
        vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error('denied'));
        const { onError } = setup();
        fireEvent.click(screen.getByRole('button', { name: '复制消息' }));
        await waitFor(() => expect(onError).toHaveBeenCalledWith('复制失败，请重试或选择消息文字后复制'));
        expect(screen.getByRole('status')).toBeEmptyDOMElement();
    });
    it('restores a failed message into the current draft without sending', async () => {
        const user = userEvent.setup();
        const { store } = setup({ ...baseMessage, id: undefined, mine: true, status: 'failed', segments: [{ type: 'text', data: { text: '失败的内容' } }] });
        const send = vi.spyOn(store, 'send');
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        expect(screen.queryByRole('menuitem', { name: '回复' })).not.toBeInTheDocument();
        await user.click(screen.getByRole('menuitem', { name: '放回输入框' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({ ...draft, text: '正在写的文字\n失败的内容' });
        expect(send).not.toHaveBeenCalled();
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
    });
    it.each([
        ['own message', { ...baseMessage, mine: true }, contact],
        ['private message', baseMessage, { ...contact, type: 'private' as const, key: 'private:20' as const }],
    ])('does not offer mention actions for a %s', async (_name, message, session) => {
        const user = userEvent.setup();
        setup(message, session);
        expect(screen.queryByRole('button', { name: '提及小明' })).not.toBeInTheDocument();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        expect(screen.queryByRole('menuitem', { name: '提及 小明' })).not.toBeInTheDocument();
    });
    it('does not expose actions for recalled messages', async () => {
        const user = userEvent.setup();
        setup({ ...baseMessage, recalled: true });
        expect(screen.queryByRole('button')).not.toBeInTheDocument();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    });
});
