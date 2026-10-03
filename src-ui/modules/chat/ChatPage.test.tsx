import type { RefObject } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ChatPage } from './ChatPage';

let store: ChatAccountStore;
const target: DebugTarget = { bot_id: 'page-test', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: false, online: false };
const friend: Contact = { key: 'private:1', type: 'private', id: '1', name: '小明' };
const group: Contact = { key: 'group:3', type: 'group', id: '3', name: '讨论群' };
vi.mock('@tanstack/react-query', () => ({ useQuery: () => ({ data: [target], isLoading: false }) }));
vi.mock('../../hooks/chat/chatStore', async importOriginal => ({ ...await importOriginal<typeof import('../../hooks/chat/chatStore')>(), chatAccount: () => store, reconcileChatAccounts: vi.fn() }));
vi.mock('../debug/BotPicker', () => ({ BotPicker: () => <span>账号</span> }));
vi.mock('./ChatDetails', () => ({ ChatDetails: () => null }));
vi.mock('./ChatDivider', () => ({ ChatDivider: () => null }));
vi.mock('./ChatAvatar', () => ({ ChatAvatar: () => <span /> }));
vi.mock('./ChatSearch', () => ({ ChatSearch: () => null }));
vi.mock('./ChatTimeline', () => ({ NativeTimeline: () => null }));
vi.mock('./ChatComposer', () => ({ ChatComposer: ({ inputRef, contact }: { inputRef: RefObject<HTMLTextAreaElement>; contact: Contact }) => <textarea aria-label="消息输入框" ref={inputRef} defaultValue={store.getSnapshot().account.drafts[contact.key]?.text ?? ''} /> }));
vi.mock('@tanstack/react-virtual', async importOriginal => ({
    ...await importOriginal<typeof import('@tanstack/react-virtual')>(),
    useVirtualizer: (options: { count: number; getItemKey: (index: number) => string }) => ({ getTotalSize: () => options.count * 68, getVirtualItems: () => Array.from({ length: options.count }, (_, index) => ({ index, key: options.getItemKey(index), start: index * 68 })), scrollToIndex: vi.fn() }),
}));

beforeEach(() => {
    store = new ChatAccountStore(target, { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() });
    store.open(group); store.open(friend);
    store.draft(friend.key, { text: '正在编辑', attachments: [], reply: null });
    vi.spyOn(store, 'initialize').mockResolvedValue(undefined);
});

describe('chat conversation shortcuts', () => {
    it.each(['Control', 'Meta'])('focuses and selects search with %s+K after revealing the list', async modifier => {
        const user = userEvent.setup(); render(<ChatPage onNavigate={vi.fn()} />);
        const input = screen.getByRole('combobox', { name: '搜索会话或联系人' });
        await user.type(input, '小明');
        await user.keyboard('{Enter}');
        await waitFor(() => expect(screen.getByRole('textbox', { name: '消息输入框' })).toHaveFocus());
        await user.keyboard(`{${modifier}>}k{/${modifier}}`);
        await waitFor(() => expect(input).toHaveFocus());
        expect((input as HTMLInputElement).selectionStart).toBe(0);
        expect((input as HTMLInputElement).selectionEnd).toBe(2);
        expect(input.closest('.native-chat-workspace')).toHaveAttribute('data-conversation', 'false');
    });
    it('leaves ordinary typing and composing shortcuts in the current input', async () => {
        const user = userEvent.setup(); render(<ChatPage onNavigate={vi.fn()} />);
        const composer = screen.getByRole('textbox', { name: '消息输入框' });
        await user.click(composer); await user.keyboard('k');
        expect(composer).toHaveFocus(); expect(composer).toHaveValue('正在编辑k');
        fireEvent.keyDown(composer, { key: 'k', ctrlKey: true, isComposing: true });
        fireEvent.keyDown(composer, { key: 'k', metaKey: true, keyCode: 229 });
        expect(composer).toHaveFocus();
    });
    it('searches private conversations from the group box and focuses the chosen draft after Enter', async () => {
        const user = userEvent.setup(); render(<ChatPage onNavigate={vi.fn()} />);
        await user.click(screen.getByRole('button', { name: /群消息盒子/ }));
        await user.click(screen.getByRole('option', { name: /讨论群/ }));
        const composer = screen.getByRole('textbox', { name: '消息输入框' });
        act(() => composer.focus()); await user.keyboard('{Control>}k{/Control}');
        const search = screen.getByRole('combobox', { name: '搜索会话或联系人' });
        await waitFor(() => expect(search).toHaveFocus());
        await user.type(search, '小明'); await user.keyboard('{ArrowDown}{Enter}');
        const chosenDraft = screen.getByRole('textbox', { name: '消息输入框' });
        await waitFor(() => expect(chosenDraft).toHaveFocus());
        expect(store.getSnapshot().account.active).toBe(friend.key);
        expect((chosenDraft as HTMLTextAreaElement).selectionStart).toBe(4);
        expect((chosenDraft as HTMLTextAreaElement).selectionEnd).toBe(4);
    });
});
