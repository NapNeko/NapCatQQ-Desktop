import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatComposer } from './ChatComposer';
import { ChatAccountStore } from '../../hooks/chat/chatStore';

function setup(disabledReason = '') {
    const store = new ChatAccountStore({ bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true });
    store.draft('private:12', { text: '你好', attachments: [], reply: null });
    const send = vi.spyOn(store, 'send').mockResolvedValue();
    render(<ChatComposer store={store} contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }} disabledReason={disabledReason} />);
    return { send, input: screen.getByRole('textbox', { name: '发送消息给好友' }) };
}
describe('native composer keyboard', () => {
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
