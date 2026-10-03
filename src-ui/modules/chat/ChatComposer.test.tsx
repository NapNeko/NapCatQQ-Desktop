import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatComposer } from './ChatComposer';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { chatMediaService } from '../../core/services/chat-media.service';

function setup(disabledReason = '', sendShortcut: 'enter' | 'ctrl-enter' = 'enter') {
    const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
    store.draft('private:12', { text: '你好', attachments: [], reply: null });
    const send = vi.spyOn(store, 'send').mockResolvedValue();
    render(<ChatComposer store={store} contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }} disabledReason={disabledReason} sendShortcut={sendShortcut} onSendShortcutChange={() => {}} />);
    return { send, input: screen.getByRole('textbox', { name: '发送消息给好友' }) };
}
describe('native composer keyboard', () => {
    it('selects QQ and favorite faces into the draft without sending', async () => {
        const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        const favorites = vi.spyOn(chatMediaService, 'favorites').mockResolvedValue(['https://cdn.example/fav.gif']);
        render(<ChatComposer store={store} contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }} disabledReason="" />);
        fireEvent.click(screen.getByRole('button', { name: '表情' }));
        expect(favorites).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '插入QQ 表情 14' }));
        expect(store.getSnapshot().account.drafts['private:12'].attachments[0]).toMatchObject({ type: 'face', id: '14' });
        fireEvent.click(screen.getByRole('button', { name: '表情' }));
        fireEvent.click(screen.getByRole('tab', { name: '收藏表情' }));
        fireEvent.click(await screen.findByRole('button', { name: '插入收藏表情 1' }));
        expect(store.getSnapshot().account.drafts['private:12'].attachments[1]).toMatchObject({ type: 'image', path: 'https://cdn.example/fav.gif', subType: 1 });
        expect(send).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '发送消息' })).toBeEnabled();
    });
    it('sends only with the selected Ctrl+Enter shortcut', () => {
        const { send, input } = setup('', 'ctrl-enter');
        fireEvent.keyDown(input, { key: 'Enter' });
        fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
        expect(send).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
        expect(send).toHaveBeenCalledOnce();
    });
    it('uses arrows and Enter to choose a member without sending', async () => {
        const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        vi.spyOn(store, 'members').mockResolvedValue([{ id: '10001', name: '小明' }, { id: '10002', name: '小李' }]);
        render(<ChatComposer store={store} contact={{ key: 'group:12', id: '12', name: '群', type: 'group' }} disabledReason="" />);
        const input = screen.getByRole('textbox', { name: '发送消息给群' });
        fireEvent.change(input, { target: { value: '@', selectionStart: 1 } });
        await screen.findByRole('option', { name: /小明/ });
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(input).toHaveValue('@小李 ');
        expect(store.getSnapshot().account.drafts['group:12'].mentions).toEqual([{ qq: '10002', label: '@小李' }]);
        expect(send).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('dismisses suggestions with Escape without changing the draft', async () => {
        const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
        vi.spyOn(store, 'members').mockResolvedValue([{ id: '10001', name: '小明' }]);
        render(<ChatComposer store={store} contact={{ key: 'group:12', id: '12', name: '群', type: 'group' }} disabledReason="" />);
        const input = screen.getByRole('textbox', { name: '发送消息给群' });
        fireEvent.change(input, { target: { value: '@没有这个人', selectionStart: 7 } });
        await screen.findByText('没有匹配的成员');
        fireEvent.keyDown(input, { key: 'Escape' });
        await waitFor(() => expect(screen.queryByRole('listbox', { name: '群成员建议' })).not.toBeInTheDocument());
        expect(input).toHaveValue('@没有这个人');
    });
    it('keeps offline editing keys when member suggestions are hidden', async () => {
        const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        vi.spyOn(store, 'members').mockResolvedValue([{ id: '10001', name: '小明' }]);
        const contact = { key: 'group:12' as const, id: '12', name: '群', type: 'group' as const };
        const { rerender } = render(<ChatComposer store={store} contact={contact} disabledReason="" />);
        const input = screen.getByRole('textbox', { name: '发送消息给群' });
        fireEvent.change(input, { target: { value: '@小', selectionStart: 2 } });
        await screen.findByRole('option', { name: /小明/ });
        rerender(<ChatComposer store={store} contact={contact} disabledReason="账号未登录" />);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(input).toHaveValue('@小');
        expect(input).not.toHaveAttribute('aria-activedescendant');
        expect(fireEvent.keyDown(input, { key: 'ArrowDown' })).toBe(true);
        expect(fireEvent.keyDown(input, { key: 'Tab' })).toBe(true);
        expect(send).not.toHaveBeenCalled();
    });
    it('does not send while confirming a Chinese IME composition', () => {
        const { send, input } = setup();
        fireEvent.compositionStart(input); fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).not.toHaveBeenCalled();
        fireEvent.compositionEnd(input); fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('keeps Shift+Enter for a newline', () => {
        const { send, input } = setup(); fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
        expect(send).not.toHaveBeenCalled();
    });
    it('keeps offline drafts editable but blocks sending', () => {
        const { send, input } = setup('账号未登录');
        fireEvent.change(input, { target: { value: '离线草稿' } });
        expect(input).toHaveValue('离线草稿');
        fireEvent.keyDown(input, { key: 'Enter' }); expect(send).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '发送消息' })).toBeDisabled();
    });
});
