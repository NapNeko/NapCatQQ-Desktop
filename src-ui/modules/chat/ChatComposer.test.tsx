import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatComposer } from './ChatComposer';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { chatMediaService } from '../../core/services/chat-media.service';
import { createRef, useState, type ReactNode } from 'react';
import { qqFaceService } from '../../core/services/qq-face.service';
import { QQ_FACE_FALLBACK } from '../../core/domain/chat/qqFaces';
import { setChatPreferences } from './chatPreferences';

beforeEach(() => {
    setChatPreferences({ composerHeight: null });
    vi.spyOn(qqFaceService, 'catalog').mockResolvedValue(QQ_FACE_FALLBACK);
});
// ChatComposer 经 useChatSend 挂 pickFile 的 useMutation，注册 mutation 需要 provider。
function ProviderWrapper({ children }: { children: ReactNode }) {
    const [client] = useState(() => new QueryClient());
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function setup(disabledReason = '') {
    const store = new ChatAccountStore({
        bot_id: 'bot',
        name: '测试',
        qq_id: 99,
        backend: 'napcat',
        host: { kind: 'local' },
        running: true,
        online: true,
    });
    store.draft('private:12', { text: '你好', attachments: [], reply: null });
    const send = vi.spyOn(store, 'send').mockResolvedValue();
    render(
        <ChatComposer
            store={store}
            contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }}
            disabledReason={disabledReason}
        />,
        { wrapper: ProviderWrapper },
    );
    return { send, input: screen.getByRole('textbox', { name: '发送消息给好友' }) };
}
describe('native composer keyboard', () => {
    it('keeps the editor, quote and attachments mounted through selection collapse and restores focus', async () => {
        setChatPreferences({ composerHeight: 120 });
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        const contact = {
            key: 'private:12' as const,
            id: '12',
            name: '好友',
            type: 'private' as const,
        };
        const draft = {
            text: '继续写',
            attachments: [{ key: 'face', type: 'face' as const, id: '14', name: '微笑' }],
            reply: { id: '1', name: '好友', preview: '引用内容' },
        };
        store.draft(contact.key, draft);
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        const inputRef = createRef<HTMLTextAreaElement>();
        const view = render(
            <ChatComposer store={store} contact={contact} inputRef={inputRef} disabledReason="" />,
            { wrapper: ProviderWrapper },
        );
        const input = screen.getByRole('textbox', { name: '发送消息给好友' });
        view.rerender(
            <ChatComposer
                store={store}
                contact={contact}
                inputRef={inputRef}
                disabledReason=""
                collapsed
            />,
        );
        expect(inputRef.current).toBe(input);
        expect(input).toHaveStyle({ height: '120px' });
        expect(input.closest('.native-chat-composer')).toHaveAttribute('inert');
        expect(input.closest('.native-chat-composer')).toHaveAttribute('aria-hidden', 'true');
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).not.toHaveBeenCalled();
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
        view.rerender(
            <ChatComposer store={store} contact={contact} inputRef={inputRef} disabledReason="" />,
        );
        expect(screen.getByRole('textbox', { name: '发送消息给好友' })).toBe(input);
        expect(input).toHaveStyle({ height: '120px' });
        expect(screen.getByRole('button', { name: '取消引用' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '移除微笑' })).toBeInTheDocument();
        await waitFor(() => expect(input).toHaveFocus());
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
    });
    it('keeps manual height through editing and restores automatic sizing on double click', () => {
        const { input, send } = setup();
        const handle = screen.getByRole('separator', { name: '调整输入框高度' });
        fireEvent.keyDown(handle, { key: 'ArrowUp' });
        expect(input).toHaveStyle({ height: '68px' });
        fireEvent.change(input, { target: { value: '调整高度后继续输入' } });
        expect(input).toHaveStyle({ height: '68px' });
        expect(JSON.parse(localStorage.getItem('ncd.chat.ui.v1')!).composerHeight).toBe(68);
        fireEvent.doubleClick(handle);
        expect(input).toHaveStyle({ height: '52px' });
        expect(JSON.parse(localStorage.getItem('ncd.chat.ui.v1')!).composerHeight).toBeNull();
        expect(input).toHaveValue('调整高度后继续输入');
        expect(send).not.toHaveBeenCalled();
    });
    it('clamps keyboard resizing to the viewport and never intercepts sending keys', () => {
        const { input, send } = setup();
        const handle = screen.getByRole('separator', { name: '调整输入框高度' });
        fireEvent.keyDown(handle, { key: 'End' });
        expect(Number.parseFloat((input as HTMLElement).style.height)).toBe(
            Number(handle.getAttribute('aria-valuemax')),
        );
        fireEvent.keyDown(handle, { key: 'ArrowUp' });
        expect(Number.parseFloat((input as HTMLElement).style.height)).toBe(
            Number(handle.getAttribute('aria-valuemax')),
        );
        fireEvent.keyDown(handle, { key: 'Home' });
        expect(input).toHaveStyle({ height: '52px' });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('returns focus to the editor after clicking send for consecutive messages', () => {
        const { send, input } = setup();
        const button = screen.getByRole('button', { name: '发送消息' });
        button.focus();
        fireEvent.click(button);
        expect(send).toHaveBeenCalledWith('private:12');
        expect(input).toHaveFocus();
    });
    it('shares the editor ref and keeps the draft when cancelling a reply', () => {
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        const attachment = { key: 'face', type: 'face' as const, id: '14', name: '微笑' };
        store.draft('private:12', {
            text: '继续写',
            attachments: [attachment],
            reply: { id: '1', name: '好友', preview: '原消息' },
        });
        const inputRef = createRef<HTMLTextAreaElement>();
        render(
            <ChatComposer
                store={store}
                contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }}
                disabledReason=""
                inputRef={inputRef}
            />,
            { wrapper: ProviderWrapper },
        );
        const button = screen.getByRole('button', { name: '取消引用' });
        button.focus();
        fireEvent.click(button);
        expect(inputRef.current).toBe(screen.getByRole('textbox', { name: '发送消息给好友' }));
        expect(inputRef.current).toHaveFocus();
        expect(store.getSnapshot().account.drafts['private:12']).toEqual({
            text: '继续写',
            attachments: [attachment],
            reply: null,
        });
    });
    it('selects QQ and favorite faces into the draft without sending', async () => {
        vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(240);
        vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(340);
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        const favorites = vi
            .spyOn(chatMediaService, 'favoriteDetails')
            .mockResolvedValue([{ url: 'https://cdn.example/fav.gif', description: '' }]);
        render(
            <ChatComposer
                store={store}
                contact={{ key: 'private:12', id: '12', name: '好友', type: 'private' }}
                disabledReason=""
            />,
            { wrapper: ProviderWrapper },
        );
        fireEvent.click(screen.getByRole('button', { name: '表情' }));
        expect(favorites).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '插入QQ 表情 14' }));
        expect(store.getSnapshot().account.drafts['private:12'].attachments[0]).toMatchObject({
            type: 'face',
            id: '14',
        });
        fireEvent.click(screen.getByRole('button', { name: '表情' }));
        fireEvent.click(screen.getByRole('tab', { name: '收藏表情' }));
        fireEvent.click(await screen.findByRole('button', { name: '插入收藏表情 1' }));
        expect(store.getSnapshot().account.drafts['private:12'].attachments[1]).toMatchObject({
            type: 'image',
            path: 'https://cdn.example/fav.gif',
            subType: 1,
        });
        expect(send).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '发送消息' })).toBeEnabled();
    });
    it('sends with Enter and leaves modified Enter alone', () => {
        const { send, input } = setup();
        fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
        fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
        expect(send).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('uses arrows and Enter to choose a member without sending', async () => {
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        vi.spyOn(store, 'members').mockResolvedValue([
            { id: '10001', name: '小明' },
            { id: '10002', name: '小李' },
        ]);
        render(
            <ChatComposer
                store={store}
                contact={{ key: 'group:12', id: '12', name: '群', type: 'group' }}
                disabledReason=""
            />,
            { wrapper: ProviderWrapper },
        );
        const input = screen.getByRole('textbox', { name: '发送消息给群' });
        fireEvent.change(input, { target: { value: '@', selectionStart: 1 } });
        await screen.findByRole('option', { name: /小明/ });
        fireEvent.keyDown(input, { key: 'ArrowDown' });
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(input).toHaveValue('@小李 ');
        expect(store.getSnapshot().account.drafts['group:12'].mentions).toEqual([
            { qq: '10002', label: '@小李' },
        ]);
        expect(send).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('dismisses suggestions with Escape without changing the draft', async () => {
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        vi.spyOn(store, 'members').mockResolvedValue([{ id: '10001', name: '小明' }]);
        render(
            <ChatComposer
                store={store}
                contact={{ key: 'group:12', id: '12', name: '群', type: 'group' }}
                disabledReason=""
            />,
            { wrapper: ProviderWrapper },
        );
        const input = screen.getByRole('textbox', { name: '发送消息给群' });
        fireEvent.change(input, { target: { value: '@没有这个人', selectionStart: 7 } });
        await screen.findByText('没有匹配的成员');
        fireEvent.keyDown(input, { key: 'Escape' });
        await waitFor(() =>
            expect(screen.queryByRole('listbox', { name: '群成员建议' })).not.toBeInTheDocument(),
        );
        expect(input).toHaveValue('@没有这个人');
    });
    it('keeps offline editing keys when member suggestions are hidden', async () => {
        const store = new ChatAccountStore({
            bot_id: 'bot',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        });
        const send = vi.spyOn(store, 'send').mockResolvedValue();
        vi.spyOn(store, 'members').mockResolvedValue([{ id: '10001', name: '小明' }]);
        const contact = { key: 'group:12' as const, id: '12', name: '群', type: 'group' as const };
        const { rerender } = render(
            <ChatComposer store={store} contact={contact} disabledReason="" />,
            { wrapper: ProviderWrapper },
        );
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
        fireEvent.compositionStart(input);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).not.toHaveBeenCalled();
        fireEvent.compositionEnd(input);
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).toHaveBeenCalledOnce();
    });
    it('keeps Shift+Enter for a newline', () => {
        const { send, input } = setup();
        fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
        expect(send).not.toHaveBeenCalled();
    });
    it('keeps offline drafts editable but blocks sending', () => {
        const { send, input } = setup('账号未登录');
        fireEvent.change(input, { target: { value: '离线草稿' } });
        expect(input).toHaveValue('离线草稿');
        fireEvent.keyDown(input, { key: 'Enter' });
        expect(send).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '发送消息' })).toBeDisabled();
    });
});
