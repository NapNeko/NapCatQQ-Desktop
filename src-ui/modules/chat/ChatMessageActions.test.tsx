import { createRef, type ReactNode } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Contact, Draft, Message } from '../../core/domain/chat/model';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { ChatMessageActions } from './ChatMessageActions';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import {
    favoriteStickerService,
    FavoriteStickerError,
} from '../../core/services/favorite-sticker.service';

// ChatMessageActions 经 useAddFavoriteSticker 走 react-query,测试树需要 Provider。
const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
const withQuery = (ui: ReactNode) => (
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>
);

const contact: Contact = { key: 'group:12', id: '12', name: '群', type: 'group' };
const baseMessage: Message = {
    key: 'group:12/2',
    session: 'group:12',
    id: '2',
    senderId: '20',
    senderName: '小明',
    at: 0,
    mine: false,
    segments: [{ type: 'text', data: { text: '  第一行\n第二行  ' } }],
    status: 'sent',
};
const draft: Draft = {
    text: '正在写的文字',
    mentions: [],
    attachments: [{ key: 'a', type: 'file', name: '备注.txt', path: 'C:/备注.txt' }],
    reply: { id: '9', name: '小李', preview: '上一条' },
};

function setup(
    message = baseMessage,
    session = contact,
    connected = false,
    selecting = false,
    selection: { selected?: boolean; disabled?: boolean; content?: ReactNode } = {},
) {
    const store = new ChatAccountStore({
        bot_id: 'bot',
        name: '测试',
        qq_id: 99,
        backend: 'napcat',
        host: { kind: 'local' },
        running: true,
        online: true,
    });
    store.draft(session.key, draft);
    if (connected) store.getSnapshot().connection = { state: 'connected' };
    const input = createRef<HTMLTextAreaElement>();
    const onFocusComposer = vi.fn(() => input.current?.focus());
    const onError = vi.fn();
    const onForward = vi.fn();
    const onSelect = vi.fn();
    const onToggleSelection = vi.fn();
    const renderMessage = (options = selection) => (
        <>
            <ChatMessageActions
                store={store}
                contact={session}
                message={message}
                onFocusComposer={onFocusComposer}
                onError={onError}
                onForward={onForward}
                onSelect={onSelect}
                selecting={selecting}
                selected={options.selected}
                selectionDisabled={options.disabled}
                onToggleSelection={onToggleSelection}
            >
                {(controls, wrapContent) => (
                    <article>
                        {wrapContent(
                            <div className="native-chat-bubble">
                                {options.content ?? <p>消息内容</p>}
                            </div>,
                        )}
                        {controls}
                    </article>
                )}
            </ChatMessageActions>
            <textarea ref={input} aria-label="输入消息" />
        </>
    );
    const view = render(withQuery(renderMessage()));
    return {
        store,
        onFocusComposer,
        onError,
        onForward,
        onSelect,
        onToggleSelection,
        rerenderSelection: (options: typeof selection) =>
            view.rerender(withQuery(renderMessage(options))),
    };
}

describe('ChatMessageActions', () => {
    it('toggles the bubble with click, Enter or Space without reopening actions or repeating held keys', async () => {
        const user = userEvent.setup();
        const { store, onToggleSelection } = setup(baseMessage, contact, true, true);
        expect(screen.queryByRole('toolbar', { name: '消息快捷操作' })).not.toBeInTheDocument();
        const bubble = screen.getByRole('button', { name: '选择这条消息' });
        bubble.focus();
        await user.keyboard('{Enter} ');
        expect(onToggleSelection).toHaveBeenCalledTimes(2);
        expect(onToggleSelection).toHaveBeenLastCalledWith(baseMessage.key);
        fireEvent.click(bubble);
        expect(onToggleSelection).toHaveBeenCalledTimes(3);
        fireEvent.keyDown(bubble, { key: ' ', repeat: true });
        expect(onToggleSelection).toHaveBeenCalledTimes(3);
        await user.keyboard('{Shift>}{F10}{/Shift}');
        expect(screen.queryByRole('menu')).not.toBeInTheDocument();
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
    });
    it('captures media, link and copy clicks as selection and keeps native controls inert', () => {
        const action = vi.fn();
        const { onToggleSelection } = setup(baseMessage, contact, true, true, {
            content: (
                <>
                    <button onPointerDown={action} onClick={action}>
                        打开图片
                    </button>
                    <video aria-label="视频" controls onClick={action} />
                    <a href="https://example.com" onClick={action}>
                        外部链接
                    </a>
                    <button onClick={action}>复制内容</button>
                </>
            ),
        });
        const media = [
            screen.getByText('打开图片'),
            screen.getByLabelText('视频'),
            screen.getByText('外部链接'),
            screen.getByText('复制内容'),
        ];
        for (const element of media) {
            expect(element.closest('[inert]')).not.toBeNull();
            fireEvent.pointerDown(element);
            fireEvent.click(element);
        }
        expect(onToggleSelection).toHaveBeenCalledTimes(4);
        expect(action).not.toHaveBeenCalled();
        expect(document.querySelector('button button')).toBeNull();
    });
    it('blocks a new selection at the limit while allowing a selected bubble to be deselected', () => {
        const { onToggleSelection, rerenderSelection } = setup(baseMessage, contact, true, true, {
            disabled: true,
        });
        const bubble = screen.getByRole('button', { name: '选择这条消息' });
        expect(bubble).toHaveAttribute('aria-disabled', 'true');
        expect(bubble).toHaveAttribute('tabindex', '-1');
        fireEvent.click(bubble);
        fireEvent.keyDown(bubble, { key: 'Enter' });
        expect(onToggleSelection).not.toHaveBeenCalled();
        rerenderSelection({ selected: true, disabled: false });
        const selected = screen.getByRole('button', { name: '取消选择这条消息' });
        expect(selected).toHaveAttribute('aria-pressed', 'true');
        expect(selected).toHaveAttribute('aria-disabled', 'false');
        fireEvent.click(selected);
        expect(onToggleSelection).toHaveBeenCalledOnce();
    });
    it('starts single and multiple forwarding from the menu without changing the draft or adding visible controls', async () => {
        const user = userEvent.setup();
        const { store, onForward, onSelect } = setup();
        expect(screen.getAllByRole('button')).toHaveLength(3);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '转发' }));
        expect(onForward).toHaveBeenCalledWith(baseMessage.key);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '多选' }));
        expect(onSelect).toHaveBeenCalledWith(baseMessage.key);
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
    });
    it('recalls a sent owned message through the same menu', async () => {
        const user = userEvent.setup();
        const { store } = setup({ ...baseMessage, mine: true, senderId: '99' }, contact, true);
        const recall = vi.spyOn(store, 'recall').mockResolvedValue();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '撤回' }));
        expect(recall).toHaveBeenCalledWith(baseMessage.key);
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
    });
    it('does not offer recall or forwarding for an uncertain send', async () => {
        const user = userEvent.setup();
        setup({ ...baseMessage, mine: true, status: 'unknown' }, contact, true);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        expect(screen.queryByRole('menuitem', { name: '撤回' })).not.toBeInTheDocument();
        expect(screen.queryByRole('menuitem', { name: '转发' })).not.toBeInTheDocument();
    });
    it('replies from the context menu using the latest draft and keeps focus in the composer', async () => {
        const user = userEvent.setup();
        const { store } = setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        store.draft(contact.key, { ...draft, text: '右键菜单打开后又写的内容' });
        await user.click(screen.getByRole('menuitem', { name: '回复' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            ...draft,
            text: '右键菜单打开后又写的内容',
            reply: { id: '2', name: '小明', preview: '  第一行\n第二行  ' },
        });
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
        expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    });
    it('mentions from the menu without losing attachments or reply', async () => {
        const user = userEvent.setup();
        const { store } = setup();
        expect(screen.getAllByRole('button')).toHaveLength(3);
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '提及 小明' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            ...draft,
            text: '正在写的文字 @小明 ',
            mentions: [{ qq: '20', label: '@小明' }],
        });
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '提及 小明' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            ...draft,
            text: '正在写的文字 @小明 @小明 ',
            mentions: [{ qq: '20', label: '@小明' }],
        });
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
    });
    it.each(['Shift+F10', 'ContextMenu'])(
        'opens message actions from a keyboard-focused control with %s',
        async (shortcut) => {
            const user = userEvent.setup();
            setup();
            screen.getByRole('button', { name: '复制消息' }).focus();
            await user.keyboard(
                shortcut === 'Shift+F10' ? '{Shift>}{F10}{/Shift}' : '{ContextMenu}',
            );
            expect(await screen.findByRole('menuitem', { name: '提及 小明' })).toBeInTheDocument();
        },
    );
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
        await waitFor(() =>
            expect(onError).toHaveBeenCalledWith('复制失败，请重试或选择消息文字后复制'),
        );
        expect(screen.getByRole('status')).toBeEmptyDOMElement();
    });
    it('restores a failed message into the current draft without sending', async () => {
        const user = userEvent.setup();
        const { store } = setup({
            ...baseMessage,
            id: undefined,
            mine: true,
            status: 'failed',
            segments: [{ type: 'text', data: { text: '失败的内容' } }],
        });
        const send = vi.spyOn(store, 'send');
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        expect(screen.queryByRole('menuitem', { name: '回复' })).not.toBeInTheDocument();
        await user.click(screen.getByRole('menuitem', { name: '放回输入框' }));
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual({
            ...draft,
            text: '正在写的文字\n失败的内容',
        });
        expect(send).not.toHaveBeenCalled();
        await waitFor(() => expect(screen.getByRole('textbox')).toHaveFocus());
    });
    it('offers a direct retry from the failed message menu without replacing the composer draft', async () => {
        const user = userEvent.setup();
        const { store } = setup(
            { ...baseMessage, id: undefined, mine: true, status: 'failed' },
            contact,
            true,
        );
        const retry = vi.spyOn(store, 'retry').mockResolvedValue();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '重新发送' }));
        expect(retry).toHaveBeenCalledWith(baseMessage.key);
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
    });
    it.each([
        ['own message', { ...baseMessage, mine: true }, contact],
        [
            'private message',
            baseMessage,
            { ...contact, type: 'private' as const, key: 'private:20' as const },
        ],
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
    it('repeats from +1 once while busy and preserves the current draft', async () => {
        const { store } = setup(baseMessage, contact, true);
        let finish!: () => void;
        const repeat = vi.spyOn(store, 'repeat').mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    finish = resolve;
                }),
        );
        const button = screen.getByRole('button', { name: '重复发送这条消息' });
        fireEvent.click(button);
        fireEvent.click(button);
        expect(repeat).toHaveBeenCalledOnce();
        expect(repeat).toHaveBeenCalledWith(baseMessage.key);
        expect(button).toBeDisabled();
        expect(store.getSnapshot().account.drafts[contact.key]).toEqual(draft);
        finish();
        await waitFor(() => expect(button).toBeEnabled());
    });
    it('announces a confirmed favorite through the global success InfoBar', async () => {
        const user = userEvent.setup();
        setup(
            {
                ...baseMessage,
                segments: [{ type: 'image', data: { url: 'https://cdn.example/face.gif' } }],
            },
            contact,
            true,
        );
        vi.spyOn(favoriteStickerService, 'add').mockResolvedValue();
        const push = vi.spyOn(globalInfoBarStore, 'push').mockReturnValue('favorite-success');
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '添加到表情' }));
        await waitFor(() =>
            expect(push).toHaveBeenCalledWith(
                expect.objectContaining({
                    tone: 'success',
                    content: '已添加到收藏表情',
                }),
            ),
        );
    });
    it('keeps an uncertain favorite result disabled instead of presenting success or resubmitting', async () => {
        const user = userEvent.setup();
        const { onError } = setup(
            {
                ...baseMessage,
                segments: [{ type: 'image', data: { url: 'https://cdn.example/face.gif' } }],
            },
            contact,
            true,
        );
        const add = vi
            .spyOn(favoriteStickerService, 'add')
            .mockRejectedValue(new FavoriteStickerError('添加结果待确认', true));
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        await user.click(screen.getByRole('menuitem', { name: '添加到表情' }));
        await waitFor(() => expect(onError).toHaveBeenCalledWith('添加结果待确认'));
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('消息内容') });
        const item = screen.getByRole('menuitem', { name: '添加结果待确认' });
        expect(item).toHaveAttribute('aria-disabled', 'true');
        fireEvent.click(item);
        expect(add).toHaveBeenCalledOnce();
    });
});
