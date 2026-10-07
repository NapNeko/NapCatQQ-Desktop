// 主窗口内的原生双栏聊天。
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import {
    ArrowLeft,
    ChevronRight,
    FolderOpen,
    Inbox,
    Mail,
    MessagesSquare,
    RefreshCw,
    Search,
    Users,
    X,
} from 'lucide-react';
import {
    chatAccount,
    reconcileChatAccounts,
    useChatSnapshot,
    restoreChatView,
    releaseChatAccount,
    selectChatBot,
    loadChatView,
} from '../../hooks/chat/chatStore';
import { useChatDesktop } from '../../hooks/chat/useChatDesktop';
import { useChatTargets } from '../../hooks/chat/useChatConversations';
import { useChatGroupSelf } from '../../hooks/chat/useChatGroupPermissions';
import { ChatAccountControls } from './ChatAccountControls';
import { ConversationSkeleton } from './ChatSkeleton';
import { useChatNotifications } from './useChatNotifications';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { accountKey, type Contact, type Message } from '../../core/domain/chat/model';
import { conversationRows, groupBoxSummary } from '../../core/domain/chat/groupBox';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ChatTrayNavigation } from '../../core/ipc/generated/chat/ChatTrayNavigation';
import { cn } from '../../shared/utils/cn';
import { ChatComposer } from './ChatComposer';
import { NativeTimeline } from './ChatTimeline';
import { BotPicker } from '../debug/BotPicker';
import { ChatAvatar as Avatar } from './ChatAvatar';
import { ChatDetails } from './ChatDetails';
import { GroupMembersDialog } from './GroupMembersDialog';
import { ChatGroupMemberMenu } from './ChatGroupMemberMenu';
import { ChatSearch } from './ChatSearch';
import { GroupFilesDialog } from './files/GroupFilesDialog';
import { ChatDivider } from './ChatDivider';
import { ConversationList, useConversationNavigation } from './ConversationList';
import { ChatContactFilters, friendCategoryItems, friendCategoryKey } from './ChatContactFilters';
import { setChatPreferences, setConversationHidden, useChatPreferences } from './chatPreferences';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui/Dialog';
import { useMotion } from '../../hooks/preferences/useMotion';
import { Button } from '../../shared/ui/Button';
import { ActionMotionIcon } from '../../shared/ui/motion/ActionMotionIcon';
import { cssEase } from '../../core/design/cssEase';
import {
    ChatUnreadBadge,
    useChatControlReset,
    useChatPaneMotion,
    useChatTabMotion,
} from './chatMotion';
import './chat.css';

let lastBot = '';
export function ChatPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
    const [selected, select] = useState(lastBot);
    const [restored, setRestored] = useState(false);
    const [navigation, setNavigation] = useState<ChatTrayNavigation | null>(null);
    const { loadView, takeTrayNavigation, onAccountSelected, selectAccount } = useChatDesktop();
    useEffect(() => {
        let alive = true;
        let ready = false;
        let queue = Promise.resolve();
        const synchronize = () => {
            queue = queue
                .catch(() => {})
                .then(async () => {
                    if (!alive || !ready) return;
                    const [view, next] = await Promise.all([loadView(), takeTrayNavigation()]);
                    if (!alive) return;
                    if (next) setNavigation(next);
                    const bot = next?.botId ?? view.selectedBot;
                    if (bot) {
                        lastBot = bot;
                        selectChatBot(bot);
                        select(bot);
                    }
                });
            return queue;
        };
        const listening = onAccountSelected(() => {
            void synchronize().catch((error) => console.warn('chat tray navigation', error));
        });
        void loadChatView()
            .then(async (view) => {
                if (!alive) return;
                restoreChatView(view);
                await listening;
                ready = true;
                if (view.selectedBot) {
                    lastBot = view.selectedBot;
                    selectChatBot(view.selectedBot);
                    select(view.selectedBot);
                }
                await synchronize();
            })
            .catch(() => {})
            .finally(() => {
                if (alive) setRestored(true);
            });
        return () => {
            alive = false;
            void listening.then((unlisten) => unlisten());
        };
    }, [loadView, takeTrayNavigation, onAccountSelected]);
    const targets = useChatTargets();
    useEffect(() => {
        if (targets.data) reconcileChatAccounts(targets.data);
    }, [targets.data]);
    const target =
        targets.data?.find((t) => t.bot_id === selected) ??
        targets.data?.find((t) => t.running) ??
        targets.data?.[0];
    useEffect(() => {
        if (target && restored) {
            selectChatBot(target.bot_id);
            void selectAccount(target.bot_id).catch((error) =>
                console.warn('chat window account', error),
            );
        }
    }, [target?.bot_id, restored, selectAccount]);
    const picker = (connected: boolean, label: string) => (
        <BotPicker
            compact
            ariaLabel="聊天账号"
            statusIndicator={
                <span
                    role="img"
                    aria-label={label}
                    title={label}
                    className={cn(
                        'h-[7px] w-[7px] shrink-0 rounded-full',
                        connected ? 'bg-success' : 'bg-danger',
                    )}
                />
            }
            targets={targets.data ?? []}
            selected={target ?? null}
            loading={targets.isLoading}
            onSelect={(botId) => {
                lastBot = botId;
                selectChatBot(botId);
                select(botId);
            }}
            onManageBots={() => onNavigate('bots')}
        />
    );
    if (!restored)
        return (
            <section className="native-chat">
                <div className="native-chat-welcome">正在恢复聊天…</div>
            </section>
        );
    return (
        <section className="native-chat">
            {target ? (
                <ChatWorkspace
                    key={accountKey(target.bot_id, String(target.qq_id))}
                    target={target}
                    picker={picker}
                    onNavigate={onNavigate}
                    navigation={navigation}
                    onTrayHandled={() => setNavigation(null)}
                />
            ) : (
                <div className="native-chat-welcome">
                    <MessagesSquare size={36} strokeWidth={1.3} />
                    <h2>
                        {targets.isLoading
                            ? '正在读取账号'
                            : targets.isError
                              ? '账号读取失败'
                              : '从一个机器人开始聊天'}
                    </h2>
                    <button
                        className="native-chat-text-button"
                        onClick={() =>
                            targets.isError ? void targets.refetch() : onNavigate('bots')
                        }
                    >
                        {targets.isError ? '重试' : '前往机器人'}
                    </button>
                </div>
            )}
        </section>
    );
}

function ChatWorkspace({
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
    // 群里新到一条文件消息，开着的群文件列表就该重读；SnowLuma 不发 group_upload 通知，只能看消息
    const fileSignal = useMemo(() => {
        for (let i = messages.length - 1; i >= 0; i--)
            if (
                messages[i].status === 'sent' &&
                messages[i].segments.some((s) => s.type === 'file')
            )
                return messages[i].key;
        return '';
    }, [messages]);
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
    const connectionLabel = !target.running
        ? '机器人已停止'
        : target.online === false
          ? '账号未登录'
          : snapshot.connection.state === 'connected'
            ? '已连接'
            : snapshot.connection.state === 'connecting'
              ? '连接中'
              : snapshot.connection.state === 'reconnecting'
                ? '正在重连'
                : '连接已断开';
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
            <aside className="native-chat-list-pane" aria-label="会话列表">
                <ChatDivider
                    width={listWidth}
                    onResize={setListWidth}
                    onCommit={(width) => setChatPreferences({ listWidth: width })}
                />
                <div
                    className="native-chat-box-heading-wrap"
                    data-open={inGroupBox && tab === 'messages'}
                    aria-hidden={!(inGroupBox && tab === 'messages')}
                    {...(!(inGroupBox && tab === 'messages') ? { inert: '' } : {})}
                >
                    <div className="native-chat-box-heading">
                        <button
                            className="native-chat-icon"
                            aria-label="返回全部会话"
                            onClick={() => {
                                setInGroupBox(false);
                                setQuery('');
                            }}
                        >
                            <ArrowLeft size={16} />
                        </button>
                        <span>群消息盒子</span>
                        <span className="ml-auto text-text-tertiary text-[11px]">
                            {box.count} 个群
                        </span>
                    </div>
                </div>
                <div className="native-chat-search">
                    <Search size={15} aria-hidden />
                    <input
                        ref={listSearchInput}
                        role="combobox"
                        aria-label="搜索会话或联系人"
                        aria-autocomplete="list"
                        aria-expanded={rows.length > 0}
                        aria-controls={rows.length ? listNavigation.listId : undefined}
                        aria-activedescendant={listNavigation.activeDescendant}
                        aria-keyshortcuts="Control+k Meta+k"
                        title="搜索会话或联系人 · Ctrl+K / ⌘K"
                        placeholder={tab === 'messages' && unread ? '搜索未读会话' : '搜索'}
                        value={query}
                        onChange={(e) => {
                            setQuery(e.target.value);
                            listNavigation.reset();
                        }}
                        onKeyDown={listNavigation.onKeyDown}
                        onBlur={listNavigation.reset}
                    />
                    {query && (
                        <button
                            aria-label="清除搜索"
                            onClick={() => {
                                setQuery('');
                                listNavigation.reset();
                                listSearchInput.current?.focus();
                            }}
                        >
                            <X size={13} />
                        </button>
                    )}
                    {tab === 'messages' && (
                        <button
                            type="button"
                            className="native-chat-list-filter"
                            aria-label="只看未读会话"
                            title={unread ? '显示全部会话' : '只看未读会话'}
                            aria-pressed={unread}
                            onClick={() => {
                                setUnread(!unread);
                                listNavigation.reset();
                            }}
                        >
                            <Mail size={14} />
                        </button>
                    )}
                    {tab === 'contacts' && (
                        <button
                            type="button"
                            className="native-chat-list-filter"
                            aria-label="刷新联系人"
                            title="刷新联系人"
                            disabled={snapshot.contactsLoading || !connected}
                            onClick={() => void store.loadContacts()}
                        >
                            <ActionMotionIcon
                                icon={RefreshCw}
                                motion={snapshot.contactsLoading ? 'spin' : 'none'}
                                size={14}
                            />
                        </button>
                    )}
                </div>
                <div className="native-chat-tabs">
                    <div role="tablist" aria-label="聊天列表">
                        {(['messages', 'contacts'] as const).map((value) => (
                            <button
                                key={value}
                                role="tab"
                                aria-selected={tab === value}
                                onClick={() => setTab(value)}
                            >
                                {value === 'messages' ? '消息' : '联系人'}
                            </button>
                        ))}
                        <span className="native-chat-tab-indicator" aria-hidden />
                    </div>
                    <Button
                        ref={accountSearchTrigger}
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label="搜索聊天记录"
                        title="搜索聊天记录"
                        onClick={() => {
                            searchRevealed.current = false;
                            setSearchExpanded(false);
                            setSearchScope('account');
                            setSearchOpen(true);
                        }}
                    >
                        <Search size={15} />
                    </Button>
                </div>
                {tab === 'contacts' && (
                    <ChatContactFilters
                        contacts={snapshot.contacts}
                        type={contactType}
                        category={category}
                        categories={categories}
                        onTypeChange={(value) => {
                            setContactType(value);
                            listNavigation.reset();
                        }}
                        onCategoryChange={(value) => {
                            setFriendCategory(value);
                            listNavigation.reset();
                        }}
                    />
                )}
                {tab === 'messages' &&
                    !inGroupBox &&
                    !query.trim() &&
                    (!unread || box.unread > 0) && (
                        <button
                            className="native-chat-box-entry"
                            onClick={() => setInGroupBox(true)}
                        >
                            <span className="native-chat-avatar is-group">
                                <Inbox size={20} />
                            </span>
                            <span className="min-w-0 flex-1 text-left">
                                <span className="text-[13px] font-medium">群消息盒子</span>
                                <span className="mt-1 block truncate text-[11px] text-text-tertiary">
                                    {box.latest
                                        ? `${box.latest.name}：${box.latest.preview || '暂无消息'}`
                                        : '暂无群消息'}
                                </span>
                            </span>
                            <ChatUnreadBadge count={box.unread} />
                            <ChevronRight size={13} className="text-text-tertiary" />
                        </button>
                    )}
                {rows.length > 0 && (
                    <ConversationList
                        key={`${tab}:${inGroupBox}:${contactType}:${category}`}
                        rows={rows}
                        active={account.active}
                        store={store}
                        onOpen={open}
                        contacts={tab === 'contacts'}
                        listId={listNavigation.listId}
                        highlighted={listNavigation.highlighted}
                        onHide={hide}
                        ignoredGroups={notifications.ignored}
                        qqMuted={notifications.qqMuted}
                        onIgnoreGroup={(contact, ignored) =>
                            void notifications.mute(contact.id, ignored)
                        }
                        notificationBusy={notifications.busy}
                    />
                )}
                {!rows.length &&
                    ((!snapshot.hydrated && !snapshot.archiveError) ||
                    snapshot.recentLoading ||
                    snapshot.contactsLoading ? (
                        <ConversationSkeleton />
                    ) : (
                        <div className="native-chat-list-empty">
                            {query ? (
                                '没有找到匹配项'
                            ) : unread && tab === 'messages' ? (
                                '没有未读消息'
                            ) : inGroupBox && tab === 'messages' ? (
                                '暂无群消息'
                            ) : tab === 'contacts' ? (
                                contactType === 'group' ? (
                                    '暂无群聊'
                                ) : category === 'all' ? (
                                    '暂无好友'
                                ) : (
                                    '该分组暂无好友'
                                )
                            ) : (
                                <>
                                    <span>{box.count ? '群会话已收进消息盒子' : '还没有会话'}</span>
                                    <button
                                        className="native-chat-text-button"
                                        onClick={() => setTab('contacts')}
                                    >
                                        从联系人开始
                                    </button>
                                </>
                            )}
                        </div>
                    ))}
                <div className="native-chat-connection">
                    <div className="native-chat-account-picker">
                        {picker(connected, connectionLabel)}
                    </div>
                    <ChatAccountControls
                        target={target}
                        contacts={snapshot.contacts}
                        connectionLabel={connectionLabel}
                        onReconnect={() =>
                            target.running ? store.initialize() : onNavigate('bots')
                        }
                    />
                </div>
            </aside>
            <main className="native-chat-message-pane" aria-label="当前会话">
                <div className="native-chat-conversation-surface">
                    {active ? (
                        <>
                            <header className="native-chat-conversation-header">
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="native-chat-back native-chat-icon"
                                    aria-label="返回会话列表"
                                    onClick={() => {
                                        setNarrowFocus(false);
                                        store.setReading(null);
                                    }}
                                >
                                    <ArrowLeft size={18} />
                                </Button>
                                <button
                                    type="button"
                                    className="native-chat-profile"
                                    aria-label={`查看${active.name}的资料`}
                                    aria-expanded={detailsOpen}
                                    onClick={() => setDetailsOpen(true)}
                                >
                                    <Avatar contact={active} small />
                                    <span className="min-w-0 text-left">
                                        <span className="block truncate text-[14px] font-semibold">
                                            {active.name}
                                        </span>
                                        <span className="block text-[11px] text-text-tertiary">
                                            {active.type === 'group'
                                                ? active.members
                                                    ? `${active.members} 位成员`
                                                    : '群聊'
                                                : active.id}
                                        </span>
                                    </span>
                                </button>
                                <Button
                                    ref={searchTrigger}
                                    variant="ghost"
                                    size="icon"
                                    className="native-chat-icon"
                                    aria-label="搜索当前会话"
                                    title="搜索消息 · Ctrl+F"
                                    aria-expanded={searchOpen}
                                    onClick={() => (searchOpen ? closeSearch() : openSearch())}
                                >
                                    <Search size={17} />
                                </Button>
                                {active.type === 'group' && (
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        className="native-chat-icon"
                                        aria-label="群成员"
                                        title="群成员"
                                        aria-expanded={membersOpen}
                                        onClick={() => showMembers()}
                                    >
                                        <Users size={17} />
                                    </Button>
                                )}
                                {active.type === 'group' && (
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        className="native-chat-icon"
                                        aria-label="群文件"
                                        title="群文件"
                                        aria-expanded={filesOpen}
                                        onClick={() => setFilesOpen(true)}
                                    >
                                        <FolderOpen size={17} />
                                    </Button>
                                )}
                                <ChatDetails
                                    key={active.key}
                                    contact={active}
                                    target={target}
                                    open={detailsOpen}
                                    onOpenChange={setDetailsOpen}
                                    onPin={() => store.pin(active.key)}
                                    onMessage={open}
                                    onSearch={openSearch}
                                    onMembers={
                                        active.type === 'group' ? () => showMembers() : undefined
                                    }
                                />
                            </header>
                            {active.type === 'group' && (
                                <GroupFilesDialog
                                    open={filesOpen}
                                    onOpenChange={setFilesOpen}
                                    target={target}
                                    groupId={active.id}
                                    groupName={active.name}
                                    connected={connected}
                                    refreshSignal={fileSignal}
                                />
                            )}
                            {active.type === 'group' && (
                                <GroupMembersDialog
                                    key={`${identity}/${active.key}`}
                                    open={membersOpen}
                                    onOpenChange={setMembersOpen}
                                    target={target}
                                    contact={active}
                                    connected={connected}
                                    onMessage={open}
                                    initialMemberId={memberFocus}
                                />
                            )}
                            <NativeTimeline
                                key={`timeline:${active.key}:${timelineEntry}`}
                                store={store}
                                contact={active}
                                messages={messages}
                                preventRecall={notifications.preventRecall}
                                revealRef={reveal}
                                visible={narrowFocus}
                                onFocusComposer={focusComposer}
                                onSelectionChange={setMessageSelecting}
                                renderAvatar={(message, avatar) => (
                                    <ChatGroupMemberMenu
                                        target={target}
                                        store={store}
                                        contact={active}
                                        message={message}
                                        onMessage={open}
                                        onViewMember={showMembers}
                                        onFocusComposer={focusComposer}
                                    >
                                        {avatar}
                                    </ChatGroupMemberMenu>
                                )}
                            />
                            <ChatComposer
                                key={`composer:${active.key}`}
                                store={store}
                                contact={active}
                                disabledReason={connected ? '' : connectionLabel}
                                inputRef={composerInput}
                                collapsed={messageSelecting}
                            />
                        </>
                    ) : (
                        <div className="native-chat-welcome">
                            <span className="native-chat-welcome-mark">
                                <MessagesSquare size={36} strokeWidth={1.25} />
                            </span>
                            <h2>开始一段对话</h2>
                            <p>选择会话，或从联系人发起聊天</p>
                            <button
                                className="native-chat-text-button"
                                onClick={() => setTab('contacts')}
                            >
                                查看联系人
                            </button>
                        </div>
                    )}
                </div>
            </main>
            <Dialog open={searchOpen} onOpenChange={setSearchOpen}>
                <DialogContent
                    size="lg"
                    hideClose
                    className="native-chat-search-dialog"
                    onCloseAutoFocus={(event) => {
                        event.preventDefault();
                        if (searchRevealed.current) {
                            searchRevealed.current = false;
                            focusComposer();
                        } else {
                            (searchScope === 'account'
                                ? accountSearchTrigger
                                : searchTrigger
                            ).current?.focus();
                        }
                    }}
                >
                    <DialogTitle className="native-chat-search-title">
                        <span>聊天记录</span>
                        {target.name}
                    </DialogTitle>
                    <ChatSearch
                        target={target}
                        initialSearchExpanded={searchExpanded}
                        inputRef={searchInput}
                        messages={messages}
                        archivedMessages={account.archiveMessages ?? account.messages}
                        conversations={account.conversations}
                        currentSession={account.active ?? undefined}
                        initialScope={searchScope}
                        preventRecall={notifications.preventRecall}
                        history={active ? snapshot.history[active.key] : undefined}
                        canLoadEarlier={connected}
                        onLoadEarlier={active ? () => void store.history(active.key) : undefined}
                        onClose={closeSearch}
                        onRevealMessage={revealSavedMessage}
                        onReveal={(key) => {
                            closeSearch();
                            requestAnimationFrame(() => reveal.current(key));
                        }}
                    />
                </DialogContent>
            </Dialog>
        </div>
    );
}
