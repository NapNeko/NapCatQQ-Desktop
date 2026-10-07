import type { RefObject } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ChatPage } from './ChatPage';
import { setChatPreferences } from './chatPreferences';
import { chatDesktopService } from '../../core/services/chat-desktop.service';

let store: ChatAccountStore;
const target: DebugTarget = {
    bot_id: 'page-test',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: false,
    online: false,
};
const friend: Contact = { key: 'private:1', type: 'private', id: '1', name: '小明' };
const group: Contact = { key: 'group:3', type: 'group', id: '3', name: '讨论群' };
// data 必须跨渲染引用稳定:真 react-query 数据未变时保持同一引用,
// 而迁移后的组件会把 query.data 镜像进本地 state,每次新数组会打满渲染循环。
vi.mock('@tanstack/react-query', () => {
    const cache: Record<string, unknown> = {};
    return {
        useQueryClient: () => ({ invalidateQueries: vi.fn() }),
        useMutation: () => ({ mutateAsync: async () => {} }),
        useQuery: ({ queryKey }: { queryKey: string[] }) =>
            queryKey[1] === 'desktop'
                ? { data: (cache.desktop ??= { accounts: [] }), isLoading: false }
                : { data: (cache.targets ??= [target]), isLoading: false },
    };
});
vi.mock('../../hooks/chat/chatStore', async (importOriginal) => ({
    ...(await importOriginal<typeof import('../../hooks/chat/chatStore')>()),
    chatAccount: () => store,
    reconcileChatAccounts: vi.fn(),
}));
vi.mock('../debug/BotPicker', () => ({ BotPicker: () => <span>账号</span> }));
vi.mock('./ChatDetails', () => ({ ChatDetails: () => null }));
vi.mock('./ChatAccountControls', () => ({ ChatAccountControls: () => null }));
vi.mock('./ChatDivider', () => ({ ChatDivider: () => null }));
vi.mock('./ChatAvatar', () => ({ ChatAvatar: () => <span /> }));
vi.mock('./ChatSearch', () => ({
    ChatSearch: ({
        onRevealMessage,
        initialScope,
    }: {
        onRevealMessage?: (message: import('../../core/domain/chat/model').Message) => void;
        initialScope?: string;
    }) => (
        <button
            onClick={() => {
                const message = store.getSnapshot().account.archiveMessages?.[0];
                if (message) onRevealMessage?.(message);
            }}
        >
            档案搜索 {initialScope}
        </button>
    ),
}));
vi.mock('./files/GroupFilesDialog', () => ({ GroupFilesDialog: () => null }));
vi.mock('./ChatTimeline', () => ({ NativeTimeline: () => null }));
vi.mock('./ChatComposer', () => ({
    ChatComposer: ({
        inputRef,
        contact,
    }: {
        inputRef: RefObject<HTMLTextAreaElement>;
        contact: Contact;
    }) => (
        <textarea
            aria-label="消息输入框"
            ref={inputRef}
            defaultValue={store.getSnapshot().account.drafts[contact.key]?.text ?? ''}
        />
    ),
}));
vi.mock('@tanstack/react-virtual', async (importOriginal) => ({
    ...(await importOriginal<typeof import('@tanstack/react-virtual')>()),
    useVirtualizer: (options: { count: number; getItemKey: (index: number) => string }) => ({
        getTotalSize: () => options.count * 68,
        getVirtualItems: () =>
            Array.from({ length: options.count }, (_, index) => ({
                index,
                key: options.getItemKey(index),
                start: index * 68,
            })),
        scrollToIndex: vi.fn(),
    }),
}));

beforeEach(() => {
    setChatPreferences({ hiddenConversations: {} });
    store = new ChatAccountStore(target, {
        call: vi.fn(),
        subscribe: vi.fn(),
        unsubscribe: vi.fn(),
    });
    store.open(group);
    store.open(friend);
    store.draft(friend.key, { text: '正在编辑', attachments: [], reply: null });
    vi.spyOn(store, 'initialize').mockResolvedValue(undefined);
});

describe('hidden conversations', () => {
    it('hides a group only from messages and reopens it from contacts with its draft intact', async () => {
        const user = userEvent.setup();
        store.draft(group.key, { text: '群草稿', attachments: [], reply: null });
        store.getSnapshot().contacts = [group, friend];
        render(<ChatPage onNavigate={vi.fn()} />);
        await screen.findByRole('combobox', { name: '搜索会话或联系人' });
        await user.click(screen.getByRole('button', { name: /群消息盒子/ }));
        const option = screen.getByRole('option', { name: /讨论群/ });
        await user.click(option);
        fireEvent.keyDown(option, { key: 'F10', shiftKey: true });
        await user.click(await screen.findByRole('menuitem', { name: '隐藏会话' }));
        expect(screen.queryByRole('option', { name: /讨论群/ })).not.toBeInTheDocument();
        expect(store.getSnapshot().account.active).toBeNull();
        expect(store.getSnapshot().account.drafts[group.key].text).toBe('群草稿');
        expect(screen.queryByRole('button', { name: /已隐藏/ })).not.toBeInTheDocument();
        await user.click(screen.getByRole('tab', { name: '联系人' }));
        await user.click(screen.getByRole('tab', { name: '群聊' }));
        await user.click(screen.getByRole('option', { name: /讨论群/ }));
        expect(store.getSnapshot().account.active).toBe(group.key);
        expect(screen.queryByRole('button', { name: '已隐藏 (1)' })).not.toBeInTheDocument();
    });
    it('keeps a hidden private conversation in contacts and restores it on click', async () => {
        const user = userEvent.setup();
        store.getSnapshot().contacts = [friend];
        render(<ChatPage onNavigate={vi.fn()} />);
        await screen.findByRole('combobox', { name: '搜索会话或联系人' });
        await user.click(screen.getByRole('tab', { name: '联系人' }));
        fireEvent.keyDown(screen.getByRole('option', { name: /小明/ }), {
            key: 'F10',
            shiftKey: true,
        });
        await user.click(await screen.findByRole('menuitem', { name: '隐藏会话' }));
        expect(screen.getByRole('option', { name: /小明/ })).toBeInTheDocument();
        await user.click(screen.getByRole('tab', { name: '消息' }));
        expect(screen.queryByRole('option', { name: /小明/ })).not.toBeInTheDocument();
        await user.click(screen.getByRole('tab', { name: '联系人' }));
        await user.click(screen.getByRole('option', { name: /小明/ }));
        expect(store.getSnapshot().account.active).toBe(friend.key);
        expect(screen.getByRole('option', { name: /小明/ })).toBeInTheDocument();
    });
});

describe('chat contact filters', () => {
    let originalScrollIntoView: PropertyDescriptor | undefined;
    beforeEach(() => {
        originalScrollIntoView = Object.getOwnPropertyDescriptor(
            Element.prototype,
            'scrollIntoView',
        );
        Object.defineProperty(Element.prototype, 'scrollIntoView', {
            configurable: true,
            writable: true,
            value: vi.fn(),
        });
    });
    afterEach(() => {
        if (originalScrollIntoView)
            Object.defineProperty(Element.prototype, 'scrollIntoView', originalScrollIntoView);
        else Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
    });
    it('separates friends and groups and filters friends by their QQ category', async () => {
        const user = userEvent.setup();
        const partner: Contact = {
            key: 'private:2',
            type: 'private',
            id: '2',
            name: '小林',
            categoryId: '2',
            categoryName: '开发伙伴',
        };
        store.getSnapshot().contacts = [
            { ...friend, categoryId: '1', categoryName: '生活朋友' },
            partner,
            group,
        ];
        render(<ChatPage onNavigate={vi.fn()} />);
        await user.click(await screen.findByRole('tab', { name: '联系人' }));
        expect(screen.getByRole('tab', { name: '好友' })).toHaveAttribute('aria-selected', 'true');
        expect(screen.queryByRole('option', { name: /讨论群/ })).not.toBeInTheDocument();
        expect(screen.getByRole('option', { name: /小明/ })).toBeInTheDocument();
        const search = screen.getByRole('combobox', { name: '搜索会话或联系人' });
        await user.click(search);
        await user.keyboard('{ArrowDown}');
        expect(search).toHaveAttribute('aria-activedescendant');
        const categorySelect = screen.getByRole('combobox', { name: '好友分组' });
        act(() => categorySelect.focus());
        await user.keyboard('{ArrowDown}');
        const categoryOption = await screen.findByRole('option', { name: '开发伙伴' });
        act(() => categoryOption.focus());
        await user.keyboard('{Enter}');
        expect(search).not.toHaveAttribute('aria-activedescendant');
        expect(screen.queryByRole('option', { name: /小明/ })).not.toBeInTheDocument();
        expect(screen.getByRole('option', { name: /小林/ })).toBeInTheDocument();
        await user.click(screen.getByRole('tab', { name: '群聊' }));
        expect(screen.queryByRole('option', { name: /小林/ })).not.toBeInTheDocument();
        expect(screen.getByRole('option', { name: /讨论群/ })).toBeInTheDocument();
        expect(screen.queryByRole('combobox', { name: '好友分组' })).not.toBeInTheDocument();
    });
    it('places unread filtering inside search and keeps empty contacts independent of it', async () => {
        const user = userEvent.setup();
        store.getSnapshot().contacts = [];
        render(<ChatPage onNavigate={vi.fn()} />);
        const unread = await screen.findByRole('button', { name: '只看未读会话' });
        expect(unread.closest('.native-chat-search')).not.toBeNull();
        await user.click(unread);
        expect(unread).toHaveAttribute('aria-pressed', 'true');
        await user.click(screen.getByRole('tab', { name: '联系人' }));
        expect(screen.getByText('暂无好友')).toBeInTheDocument();
        expect(screen.queryByText('没有未读消息')).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '只看未读会话' })).not.toBeInTheDocument();
    });
});

describe('chat conversation shortcuts', () => {
    it('opens account history without an active conversation and restores an archived result without losing drafts', async () => {
        const user = userEvent.setup();
        const saved = {
            key: 'group:3/saved',
            session: group.key,
            id: 'saved',
            senderId: '2',
            senderName: '小林',
            mine: false,
            at: 1000,
            status: 'sent' as const,
            segments: [{ type: 'text', data: { text: '已保存消息' } }],
        };
        store.getSnapshot().account.archiveMessages = [saved];
        store.getSnapshot().account.active = null;
        store.draft(group.key, { text: '群草稿', attachments: [], reply: null });
        render(<ChatPage onNavigate={vi.fn()} />);
        await user.click(await screen.findByRole('button', { name: '搜索聊天记录' }));
        await user.click(await screen.findByRole('button', { name: '档案搜索 account' }));
        expect(store.getSnapshot().account.active).toBe(group.key);
        expect(
            store.getSnapshot().account.messages.some((message) => message.key === saved.key),
        ).toBe(true);
        expect(store.getSnapshot().account.drafts[friend.key].text).toBe('正在编辑');
        expect(store.getSnapshot().account.drafts[group.key].text).toBe('群草稿');
    });
    it('opens a tray conversation after archive hydration without replacing other drafts', async () => {
        store.getSnapshot().hydrated = true;
        store.draft(group.key, { text: '群草稿', reply: null, attachments: [] });
        vi.spyOn(chatDesktopService, 'takeTrayNavigation').mockResolvedValueOnce({
            v: 1,
            botId: target.bot_id,
            selfId: '99',
            conversation: {
                ...group,
                unread: 2,
                pinned: false,
                lastAt: 1,
                preview: '新消息',
                boxed: false,
            },
        });
        render(<ChatPage onNavigate={vi.fn()} />);
        await waitFor(() => expect(store.getSnapshot().account.active).toBe(group.key));
        expect(store.getSnapshot().account.drafts[friend.key].text).toBe('正在编辑');
        expect(store.getSnapshot().account.drafts[group.key].text).toBe('群草稿');
        expect(
            screen.getByRole('textbox', { name: '消息输入框' }).closest('.native-chat-workspace'),
        ).toHaveAttribute('data-conversation', 'true');
    });
    it('reveals the handed-off group and its draft immediately in a narrow window', async () => {
        store.open(group);
        store.draft(group.key, { text: '交接群草稿', reply: null, attachments: [] });
        render(<ChatPage onNavigate={vi.fn()} />);
        const composer = await screen.findByRole('textbox', { name: '消息输入框' });
        expect(composer).toHaveValue('交接群草稿');
        expect(composer.closest('.native-chat-workspace')).toHaveAttribute(
            'data-conversation',
            'true',
        );
        expect(screen.getByRole('option', { name: /讨论群/ })).toHaveAttribute(
            'aria-current',
            'true',
        );
    });
    it.each(['Control', 'Meta'])(
        'focuses and selects search with %s+K after revealing the list',
        async (modifier) => {
            const user = userEvent.setup();
            render(<ChatPage onNavigate={vi.fn()} />);
            const input = await screen.findByRole('combobox', { name: '搜索会话或联系人' });
            await user.type(input, '小明');
            await user.keyboard('{Enter}');
            await waitFor(() =>
                expect(screen.getByRole('textbox', { name: '消息输入框' })).toHaveFocus(),
            );
            await user.keyboard(`{${modifier}>}k{/${modifier}}`);
            await waitFor(() => expect(input).toHaveFocus());
            expect((input as HTMLInputElement).selectionStart).toBe(0);
            expect((input as HTMLInputElement).selectionEnd).toBe(2);
            expect(input.closest('.native-chat-workspace')).toHaveAttribute(
                'data-conversation',
                'false',
            );
        },
    );
    it('leaves ordinary typing and composing shortcuts in the current input', async () => {
        const user = userEvent.setup();
        render(<ChatPage onNavigate={vi.fn()} />);
        const composer = await screen.findByRole('textbox', { name: '消息输入框' });
        await user.click(composer);
        await user.keyboard('k');
        expect(composer).toHaveFocus();
        expect(composer).toHaveValue('正在编辑k');
        fireEvent.keyDown(composer, { key: 'k', ctrlKey: true, isComposing: true });
        fireEvent.keyDown(composer, { key: 'k', metaKey: true, keyCode: 229 });
        expect(composer).toHaveFocus();
    });
    it('searches private conversations from the group box and focuses the chosen draft after Enter', async () => {
        const user = userEvent.setup();
        render(<ChatPage onNavigate={vi.fn()} />);
        await screen.findByRole('combobox', { name: '搜索会话或联系人' });
        await user.click(screen.getByRole('button', { name: /群消息盒子/ }));
        await user.click(screen.getByRole('option', { name: /讨论群/ }));
        const composer = screen.getByRole('textbox', { name: '消息输入框' });
        act(() => composer.focus());
        await user.keyboard('{Control>}k{/Control}');
        const search = screen.getByRole('combobox', { name: '搜索会话或联系人' });
        await waitFor(() => expect(search).toHaveFocus());
        await user.type(search, '小明');
        await user.keyboard('{ArrowDown}{Enter}');
        const chosenDraft = screen.getByRole('textbox', { name: '消息输入框' });
        await waitFor(() => expect(chosenDraft).toHaveFocus());
        expect(store.getSnapshot().account.active).toBe(friend.key);
        expect((chosenDraft as HTMLTextAreaElement).selectionStart).toBe(4);
        expect((chosenDraft as HTMLTextAreaElement).selectionEnd).toBe(4);
    });
});
