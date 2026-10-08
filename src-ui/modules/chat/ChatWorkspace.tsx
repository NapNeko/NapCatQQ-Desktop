// 聊天工作区：持有双栏的全部会话状态与动作，渲染只组合三个子栏，DOM 结构与拆分前一致。
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { chatAccount, releaseChatAccount, useChatSnapshot } from '../../hooks/chat/chatStore';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { useChatGroupSelf } from '../../hooks/chat/useChatGroupPermissions';
import { accountKey, type Contact, type Message } from '../../core/domain/chat/model';
import { conversationRows, groupBoxSummary } from '../../core/domain/chat/groupBox';
import { chatConnectionLabel } from '../../core/domain/chat/connectionLabel';
import { latestFileMessageKey } from '../../core/domain/chat/fileMessageSignal';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ChatTrayNavigation } from '../../core/ipc/generated/chat/ChatTrayNavigation';
import { cssEase } from '../../core/design/cssEase';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useChatControlReset, useChatPaneMotion, useChatTabMotion } from './chatMotion';
import { useChatNotifications } from './useChatNotifications';
import { setConversationHidden, useChatPreferences } from './chatPreferences';
import { friendCategoryItems, friendCategoryKey } from './ChatContactFilters';
import { useConversationNavigation } from './ConversationList';
import { ChatListPane } from './ChatListPane';
import { ChatConversationPane } from './ChatConversationPane';
import { ChatSearchDialog } from './ChatSearchDialog';

export function ChatWorkspace({
    target,
    picker,
    onNavigate,
    navigation,
    onTrayHandled,
}: {
    target: DebugTarget;
    picker: (connected: boolean, label: string) => ReactNode;
    onNavigate: (route: AppRoute) => void;
    navigation: ChatTrayNavigation | null;
    onTrayHandled: () => void;
}) {
    const store = chatAccount(target);
    const snapshot = useChatSnapshot(store);
    const { account } = snapshot;
    const preferences = useChatPreferences();
    const motion = useMotion();
    const identity = accountKey(target.bot_id, String(target.qq_id));
    useChatNotice(
        `${identity}:archive`,
        `${target.name} · 聊天记录异常`,
        snapshot.archiveError,
        () => void (snapshot.hydrated ? store.flushArchive() : store.restore()),
    );
    useChatNotice(
        `${identity}:recent`,
        `${target.name} · 最近会话同步失败`,
        snapshot.recentError,
        () => void store.loadRecent(true),
        'warning',
    );
    useChatNotice(
        `${identity}:connection`,
        `${target.name} · 聊天异常`,
        snapshot.error,
        () => void store.initialize(),
    );
    const localHidden = preferences.hiddenConversations[identity] ?? [];
    const notifications = useChatNotifications(target, localHidden);
    const hidden = useMemo(
        () => new Set([...localHidden, ...notifications.hidden]),
        [preferences.hiddenConversations, identity, notifications.hidden],
    );
    const workspace = useRef<HTMLDivElement>(null);
    useChatControlReset(workspace);
    const [listWidth, setListWidth] = useState(preferences.listWidth);
    const [tab, setTab] = useState<'messages' | 'contacts'>('messages');
    const [contactType, setContactType] = useState<Contact['type']>('private');
    const [friendCategory, setFriendCategory] = useState('all');
    useChatTabMotion(workspace, tab);
    const [inGroupBox, setInGroupBox] = useState(
        () => account.active?.startsWith('group:') ?? false,
    );
    const [query, setQuery] = useState('');
    const [unread, setUnread] = useState(false);
    const [searchOpen, setSearchOpen] = useState(false);
    const [searchExpanded, setSearchExpanded] = useState(false);
    const [searchScope, setSearchScope] = useState<'conversation' | 'account'>('conversation');
    const [narrowFocus, setNarrowFocus] = useState(() => !!account.active);
    const [detailsOpen, setDetailsOpen] = useState(false);
    const [filesOpen, setFilesOpen] = useState(false);
    const [membersOpen, setMembersOpen] = useState(false);
    const [memberFocus, setMemberFocus] = useState<string>();
    const [timelineEntry, setTimelineEntry] = useState(0);
    const [messageSelecting, setMessageSelecting] = useState(false);
    const searchTrigger = useRef<HTMLButtonElement>(null);
    const accountSearchTrigger = useRef<HTMLButtonElement>(null);
    const searchInput = useRef<HTMLInputElement>(null);
    const listSearchInput = useRef<HTMLInputElement>(null);
    const composerInput = useRef<HTMLTextAreaElement>(null);
    const reveal = useRef<(key: string) => void>(() => {});
    const pendingReveal = useRef<Message | null>(null);
    const searchRevealed = useRef(false);
    const active = account.active ? account.conversations[account.active] : undefined;
    useChatPaneMotion(
        workspace,
        `${tab}/${inGroupBox}/${unread}/${narrowFocus}`,
        '.native-chat-conversations, .native-chat-list-empty',
    );
    useChatPaneMotion(
        workspace,
        `${account.active ?? ''}/${narrowFocus}`,
        '.native-chat-conversation-surface',
    );
    const messages = useMemo(
        () => account.messages.filter((m) => m.session === account.active),
        [account.messages, account.active],
    );
    useEffect(() => {
        void store.initialize();
    }, [store, target.running, target.online]);
    useEffect(
        () => () => {
            store.setReading(null);
            void releaseChatAccount(store).catch((error) => console.warn('chat release', error));
        },
        [store],
    );
    useEffect(() => {
        setSearchOpen(false);
        setDetailsOpen(false);
        setFilesOpen(false);
        setMembersOpen(false);
        setMemberFocus(undefined);
        setMessageSelecting(false);
    }, [account.active]);
    const fileSignal = useMemo(() => latestFileMessageKey(messages), [messages]);
    useEffect(() => {
        if (searchOpen) searchInput.current?.focus();
    }, [searchOpen]);
    useEffect(() => {
        const message = pendingReveal.current;
        if (!message || account.active !== message.session) return;
        const frame = requestAnimationFrame(() => {
            reveal.current(message.key);
            pendingReveal.current = null;
        });
        return () => cancelAnimationFrame(frame);
    }, [account.active, timelineEntry, messages]);
    const categories = useMemo(() => friendCategoryItems(snapshot.contacts), [snapshot.contacts]);
    const category = categories.some((item) => item.value === friendCategory)
        ? friendCategory
        : 'all';
    const rows = useMemo(() => {
        const term = query.trim().toLocaleLowerCase();
        if (tab === 'messages')
            return conversationRows(
                Object.values(account.conversations).filter((c) => !hidden.has(c.key)),
                { box: inGroupBox, unread, query },
            );
        return snapshot.contacts
            .filter(
                (c) =>
                    c.type === contactType &&
                    (contactType !== 'private' ||
                        category === 'all' ||
                        friendCategoryKey(c) === category) &&
                    (!term || `${c.name} ${c.id}`.toLocaleLowerCase().includes(term)),
            )
            .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));
    }, [
        query,
        tab,
        unread,
        inGroupBox,
        snapshot.contacts,
        account.conversations,
        hidden,
        contactType,
        category,
    ]);
    const box = useMemo(
        () =>
            groupBoxSummary(Object.values(account.conversations).filter((c) => !hidden.has(c.key))),
        [account.conversations, hidden],
    );
    const connected =
        target.running && target.online !== false && snapshot.connection.state === 'connected';
    // 自身群权限的订阅与清理收敛到 useChatGroupSelf：群会话且已连接时 self + 作用域内 clear，
    // 其余情况传 null 复现原 effect 的整单 clear；依赖覆盖 identity/active.key/connected/backend。
    useChatGroupSelf(target, connected && active?.type === 'group' ? active.id : null, connected);
    const connectionLabel = chatConnectionLabel(target, snapshot.connection.state);
    const open = (contact: Contact) => {
        if (contact.type === 'group' && hidden.has(contact.key))
            void notifications.mute(contact.id, false, true);
        setConversationHidden(identity, contact.key, false);
        store.open(contact);
        setTimelineEntry((value) => value + 1);
        setInGroupBox(contact.type === 'group');
        setTab('messages');
        setNarrowFocus(true);
    };
    const hide = (contact: Contact) => {
        if (contact.type === 'group') void notifications.mute(contact.id, true, true);
        setConversationHidden(identity, contact.key, true);
        store.close(contact.key);
        setNarrowFocus(false);
    };
    const focusComposer = () =>
        requestAnimationFrame(() => {
            const input = composerInput.current;
            if (input?.isConnected && input.closest('[data-collapsed="true"]') === null) {
                input.focus();
                input.setSelectionRange(input.value.length, input.value.length);
            }
        });
    const showMembers = (memberId?: string) => {
        setMemberFocus(memberId);
        setMembersOpen(true);
        setDetailsOpen(false);
    };
    const handledNavigation = useRef<ChatTrayNavigation | null>(null);
    useEffect(() => {
        if (
            !snapshot.hydrated ||
            !navigation ||
            handledNavigation.current === navigation ||
            navigation.botId !== target.bot_id ||
            navigation.selfId !== String(target.qq_id)
        )
            return;
        handledNavigation.current = navigation;
        const conversation = navigation.conversation;
        open({ ...conversation, key: `${conversation.type}:${conversation.id}` });
        focusComposer();
        onTrayHandled();
    }, [navigation, snapshot.hydrated, target.bot_id, target.qq_id]);
    const listNavigation = useConversationNavigation(rows, (contact) => {
        open(contact);
        focusComposer();
    });
    const closeSearch = () => {
        setSearchOpen(false);
        (searchScope === 'account' ? accountSearchTrigger : searchTrigger).current?.focus();
    };
    const openSearch = (typing = false) => {
        searchRevealed.current = false;
        setSearchExpanded(typing);
        setSearchScope('conversation');
        setSearchOpen(true);
        setNarrowFocus(true);
        searchInput.current?.focus();
    };
    const revealSavedMessage = (message: Message) => {
        const saved = store.revealArchivedMessage(message);
        if (!saved) return;
        const contact = store.getSnapshot().account.conversations[saved.session];
        if (contact?.type === 'group' && hidden.has(contact.key))
            void notifications.mute(contact.id, false, true);
        setConversationHidden(identity, saved.session, false);
        pendingReveal.current = saved;
        searchRevealed.current = true;
        setTimelineEntry((value) => value + 1);
        setInGroupBox(saved.session.startsWith('group:'));
        setTab('messages');
        setNarrowFocus(true);
        setSearchOpen(false);
    };
    return (
        <div
            ref={workspace}
            className="native-chat-workspace"
            style={
                {
                    '--chat-list-width': `${listWidth}px`,
                    '--chat-motion-fast': `${motion.duration('fast')}s`,
                    '--chat-motion-ease': cssEase(motion.ease.damped),
                    '--chat-action-shift': `${motion.level === 'elegant' ? 0 : motion.level === 'rich' ? 6 : 3}px`,
                } as CSSProperties
            }
            data-conversation={narrowFocus}
            data-motion={motion.enabled ? motion.level : 'off'}
            onKeyDown={(e) => {
                if (
                    e.defaultPrevented ||
                    e.nativeEvent.isComposing ||
                    e.keyCode === 229 ||
                    e.altKey
                )
                    return;
                if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
                    e.preventDefault();
                    setNarrowFocus(false);
                    setInGroupBox(false);
                    store.setReading(null);
                    requestAnimationFrame(() => {
                        const input = listSearchInput.current;
                        if (input?.isConnected) {
                            input.focus();
                            input.select();
                        }
                    });
                }
                if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f' && active) {
                    e.preventDefault();
                    openSearch(true);
                }
            }}
        >
            <ChatListPane
                target={target}
                store={store}
                snapshot={snapshot}
                ignoredGroups={notifications.ignored}
                qqMuted={notifications.qqMuted}
                notificationBusy={notifications.busy}
                onMuteGroup={(groupId, ignored) => void notifications.mute(groupId, ignored)}
                connected={connected}
                connectionLabel={connectionLabel}
                picker={picker}
                onNavigate={onNavigate}
                listWidth={listWidth}
                onListWidthChange={setListWidth}
                tab={tab}
                onTabChange={setTab}
                inGroupBox={inGroupBox}
                onGroupBoxChange={setInGroupBox}
                query={query}
                onQueryChange={setQuery}
                unread={unread}
                onUnreadChange={setUnread}
                contactType={contactType}
                onContactTypeChange={setContactType}
                category={category}
                onCategoryChange={setFriendCategory}
                categories={categories}
                rows={rows}
                active={account.active}
                box={box}
                listSearchInput={listSearchInput}
                accountSearchTrigger={accountSearchTrigger}
                onOpenAccountSearch={() => {
                    searchRevealed.current = false;
                    setSearchExpanded(false);
                    setSearchScope('account');
                    setSearchOpen(true);
                }}
                onOpen={open}
                onHide={hide}
                listNavigation={listNavigation}
            />
            <ChatConversationPane
                target={target}
                store={store}
                contact={active}
                identity={identity}
                messages={messages}
                timelineEntry={timelineEntry}
                connected={connected}
                connectionLabel={connectionLabel}
                fileSignal={fileSignal}
                narrowFocus={narrowFocus}
                messageSelecting={messageSelecting}
                detailsOpen={detailsOpen}
                onDetailsOpenChange={setDetailsOpen}
                searchOpen={searchOpen}
                searchTrigger={searchTrigger}
                membersOpen={membersOpen}
                onMembersOpenChange={setMembersOpen}
                filesOpen={filesOpen}
                onFilesOpenChange={setFilesOpen}
                memberFocus={memberFocus}
                composerInput={composerInput}
                reveal={reveal}
                preventRecall={notifications.preventRecall}
                onBack={() => {
                    setNarrowFocus(false);
                    store.setReading(null);
                }}
                onOpenSearch={openSearch}
                onCloseSearch={closeSearch}
                onShowMembers={showMembers}
                onOpen={open}
                onFocusComposer={focusComposer}
                onSelectionChange={setMessageSelecting}
                onShowContacts={() => setTab('contacts')}
            />
            <ChatSearchDialog
                open={searchOpen}
                onOpenChange={setSearchOpen}
                target={target}
                account={account}
                messages={messages}
                searchExpanded={searchExpanded}
                searchScope={searchScope}
                searchInput={searchInput}
                searchTrigger={searchTrigger}
                accountSearchTrigger={accountSearchTrigger}
                searchRevealed={searchRevealed}
                history={active ? snapshot.history[active.key] : undefined}
                canLoadEarlier={connected}
                onLoadEarlier={active ? () => void store.history(active.key) : undefined}
                preventRecall={notifications.preventRecall}
                onClose={closeSearch}
                onFocusComposer={focusComposer}
                onRevealSavedMessage={revealSavedMessage}
                onReveal={(key) => {
                    closeSearch();
                    requestAnimationFrame(() => reveal.current(key));
                }}
            />
        </div>
    );
}
