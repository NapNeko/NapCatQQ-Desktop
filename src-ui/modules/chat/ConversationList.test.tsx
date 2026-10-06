import { useState } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { archiveOf } from '../../core/domain/chat/archive';
import { emptyAccount, ingestMessage, type Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ConversationList, useConversationNavigation } from './ConversationList';

const { scrollToIndex } = vi.hoisted(() => ({ scrollToIndex: vi.fn() }));
vi.mock('@tanstack/react-virtual', async (importOriginal) => {
    const original = await importOriginal<typeof import('@tanstack/react-virtual')>();
    return {
        ...original,
        useVirtualizer: (options: {
            count: number;
            getItemKey: (index: number) => string;
            rangeExtractor: (range: {
                startIndex: number;
                endIndex: number;
                overscan: number;
                count: number;
            }) => number[];
        }) => ({
            getTotalSize: () => options.count * 68,
            getVirtualItems: () =>
                options
                    .rangeExtractor({
                        startIndex: 0,
                        endIndex: Math.min(2, options.count - 1),
                        overscan: 0,
                        count: options.count,
                    })
                    .map((index) => ({ index, start: index * 68, key: options.getItemKey(index) })),
            scrollToIndex,
        }),
    };
});
vi.mock('./ChatAvatar', () => ({ ChatAvatar: () => <span /> }));
const target: DebugTarget = {
    bot_id: 'conversation-list',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: false,
    online: false,
};
async function setup() {
    const first: Contact = { key: 'private:1', type: 'private', id: '1', name: '小明' };
    const account = ingestMessage(emptyAccount('99'), {
        self_id: 99,
        user_id: 2,
        message_type: 'private',
        message_id: 1,
        time: new Date(2026, 9, 1, 9, 5).getTime() / 1000,
        sender: { nickname: '小红' },
        message: '你好',
    });
    account.conversations['private:2'] = { ...account.conversations['private:2'], unread: 3 };
    const archive = { load: vi.fn(async () => archiveOf(account)), save: vi.fn(async () => {}) };
    const store = new ChatAccountStore(
        target,
        { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
        archive,
    );
    await store.restore();
    store.open(first);
    store.draft(first.key, { text: '未发送的草稿', reply: null, attachments: [] });
    await store.flushArchive();
    return {
        store,
        rows: [first, store.getSnapshot().account.conversations['private:2']],
        onOpen: vi.fn(),
    };
}

function SearchHarness({
    rows,
    store,
    onOpen,
}: {
    rows: Contact[];
    store: ChatAccountStore;
    onOpen: (contact: Contact) => void;
}) {
    const [query, setQuery] = useState('');
    const matches = rows.filter((row) => row.name.includes(query));
    const navigation = useConversationNavigation(matches, onOpen);
    return (
        <>
            <input
                aria-label="搜索会话"
                role="combobox"
                aria-expanded={!!matches.length}
                aria-controls={navigation.listId}
                aria-activedescendant={navigation.activeDescendant}
                value={query}
                onChange={(event) => {
                    setQuery(event.target.value);
                    navigation.reset();
                }}
                onKeyDown={navigation.onKeyDown}
            />
            <ConversationList
                rows={matches}
                store={store}
                onOpen={onOpen}
                active={null}
                contacts={false}
                listId={navigation.listId}
                highlighted={navigation.highlighted}
            />
        </>
    );
}

describe('conversation list actions and navigation', () => {
    it('distinguishes QQ mute from the local ignore setting while retaining unread messages', async () => {
        const user = userEvent.setup();
        const { store } = await setup();
        const group: Contact = { key: 'group:123', type: 'group', id: '123', name: '测试群' };
        store.open(group);
        store.getSnapshot().account.conversations[group.key].unread = 5;
        const ignore = vi.fn();
        render(
            <ConversationList
                rows={[group]}
                active={null}
                store={store}
                onOpen={vi.fn()}
                contacts={false}
                listId="groups"
                highlighted={null}
                qqMuted={new Map([['123', true]])}
                ignoredGroups={new Set()}
                onIgnoreGroup={ignore}
            />,
        );
        const option = screen.getByRole('option', { name: /测试群/ });
        expect(screen.getByLabelText('消息免打扰')).toBeInTheDocument();
        await user.pointer({ keys: '[MouseRight]', target: option });
        expect(screen.queryByText(/QQ 免打扰：/)).not.toBeInTheDocument();
        await user.click(screen.getByRole('menuitemcheckbox', { name: '本地免打扰' }));
        expect(ignore).toHaveBeenCalledWith(group, true);
        expect(store.getSnapshot().account.conversations[group.key].unread).toBe(5);
    });
    it('opens the shared context menu without switching conversations and updates pin/read state', async () => {
        const user = userEvent.setup();
        const { store, rows, onOpen } = await setup();
        render(
            <ConversationList
                rows={rows}
                active="private:1"
                store={store}
                onOpen={onOpen}
                contacts={false}
                listId="conversations"
                highlighted={null}
            />,
        );
        const option = screen.getByRole('option', { name: /小红/ });
        await user.pointer({ keys: '[MouseRight]', target: option });
        await user.click(await screen.findByRole('menuitem', { name: '置顶会话' }));
        expect(store.getSnapshot().account.conversations['private:2'].pinned).toBe(true);
        await user.pointer({ keys: '[MouseRight]', target: option });
        expect(await screen.findByRole('menuitem', { name: '取消置顶' })).toBeInTheDocument();
        await user.click(screen.getByRole('menuitem', { name: '标为已读' }));
        expect(store.getSnapshot().account.conversations['private:2'].unread).toBe(0);
        expect(store.getSnapshot().account.active).toBe('private:1');
        expect(store.getSnapshot().account.drafts['private:1'].text).toBe('未发送的草稿');
        expect(onOpen).not.toHaveBeenCalled();
    });
    it('supports Shift+F10 and omits actions that do not apply to a read conversation', async () => {
        const user = userEvent.setup();
        const { store, rows, onOpen } = await setup();
        render(
            <ConversationList
                rows={rows}
                active="private:1"
                store={store}
                onOpen={onOpen}
                contacts={false}
                listId="conversations"
                highlighted={null}
            />,
        );
        act(() => screen.getByRole('option', { name: /小明/ }).focus());
        await user.keyboard('{Shift>}{F10}{/Shift}');
        expect(await screen.findByRole('menu')).toHaveClass('ndf-context-menu-content');
        expect(screen.queryByRole('menuitem', { name: '标为已读' })).not.toBeInTheDocument();
        await user.keyboard('{ArrowDown}{Enter}');
        expect(store.getSnapshot().account.conversations['private:1'].pinned).toBe(true);
    });
    it('scrolls to keyboard results outside the visible range and keeps ARIA linked to a mounted option', async () => {
        const user = userEvent.setup();
        const { store, onOpen } = await setup();
        const rows: Contact[] = Array.from({ length: 30 }, (_, index) => ({
            key: `private:${index}`,
            type: 'private',
            id: String(index),
            name: `联系人${index}`,
        }));
        render(<SearchHarness rows={rows} store={store} onOpen={onOpen} />);
        const input = screen.getByRole('combobox');
        await user.click(input);
        await user.keyboard('{ArrowUp}');
        expect(scrollToIndex).toHaveBeenLastCalledWith(29, { align: 'auto' });
        const option = screen.getByRole('option', { name: /联系人29/ });
        expect(input).toHaveAttribute('aria-activedescendant', option.id);
        expect(option).toHaveAttribute('aria-selected', 'true');
        await user.keyboard('{ArrowUp}{ArrowDown}{Enter}');
        expect(onOpen).toHaveBeenCalledWith(rows[29]);
    });
    it('selects only matching results and ignores arrows and Enter while composing', async () => {
        const user = userEvent.setup();
        const { store, rows, onOpen } = await setup();
        render(<SearchHarness rows={rows} store={store} onOpen={onOpen} />);
        const input = screen.getByRole('combobox');
        await user.type(input, '小');
        fireEvent.keyDown(input, { key: 'ArrowDown', isComposing: true });
        fireEvent.keyDown(input, { key: 'Enter', keyCode: 229 });
        expect(input).not.toHaveAttribute('aria-activedescendant');
        expect(onOpen).not.toHaveBeenCalled();
        await user.keyboard('{ArrowDown}{ArrowDown}');
        await user.type(input, '明');
        expect(input).not.toHaveAttribute('aria-activedescendant');
        await user.keyboard('{Enter}');
        expect(onOpen).toHaveBeenCalledWith(rows[0]);
        await user.clear(input);
        await user.type(input, '不存在{Enter}');
        expect(onOpen).toHaveBeenCalledTimes(1);
    });
    it('exposes full time metadata on dated conversation rows', async () => {
        const { store, rows, onOpen } = await setup();
        render(
            <ConversationList
                rows={rows}
                active={null}
                store={store}
                onOpen={onOpen}
                contacts={false}
                listId="conversations"
                highlighted={null}
            />,
        );
        const date = screen.getByTitle('2026/10/1 09:05:00');
        expect(date.tagName).toBe('TIME');
        expect(date).toHaveAttribute('datetime', new Date(2026, 9, 1, 9, 5).toISOString());
    });
});
