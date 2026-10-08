// 会话列表栏：搜索、消息/联系人 Tab、群盒子入口、列表与底部连接区，全部状态仍由 ChatWorkspace 持有。
import { type KeyboardEvent, type ReactNode, type RefObject } from 'react';
import { ArrowLeft, ChevronRight, Inbox, Mail, RefreshCw, Search, X } from 'lucide-react';
import type { ChatAccountStore, ChatSnapshot } from '../../hooks/chat/chatStore';
import type { Contact, Conversation, SessionKey } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { SelectItem } from '../../shared/ui/Select';
import { Button } from '../../shared/ui/Button';
import { ActionMotionIcon } from '../../shared/ui/motion/ActionMotionIcon';
import { ChatAccountControls } from './ChatAccountControls';
import { ConversationSkeleton } from './ChatSkeleton';
import { ChatContactFilters } from './ChatContactFilters';
import { ConversationList } from './ConversationList';
import { ChatDivider } from './ChatDivider';
import { setChatPreferences } from './chatPreferences';
import { ChatUnreadBadge } from './chatMotion';

// ConversationList 的键盘导航句柄形状，与 useConversationNavigation 的返回一致。
export interface ChatListNavigation {
    listId: string;
    highlighted: SessionKey | null;
    activeDescendant: string | undefined;
    reset: () => void;
    onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
}

// groupBoxSummary 的返回形状。
export interface ChatBoxSummary {
    count: number;
    unread: number;
    latest: Conversation | undefined;
}

interface ChatListPaneProps {
    target: DebugTarget;
    store: ChatAccountStore;
    snapshot: ChatSnapshot;
    ignoredGroups: Set<string>;
    qqMuted: Map<string, boolean | null>;
    notificationBusy: boolean;
    onMuteGroup: (groupId: string, ignored: boolean) => void;
    connected: boolean;
    connectionLabel: string;
    picker: (connected: boolean, label: string) => ReactNode;
    onNavigate: (route: AppRoute) => void;
    listWidth: number;
    onListWidthChange: (width: number) => void;
    tab: 'messages' | 'contacts';
    onTabChange: (tab: 'messages' | 'contacts') => void;
    inGroupBox: boolean;
    onGroupBoxChange: (open: boolean) => void;
    query: string;
    onQueryChange: (query: string) => void;
    unread: boolean;
    onUnreadChange: (unread: boolean) => void;
    contactType: Contact['type'];
    onContactTypeChange: (type: Contact['type']) => void;
    category: string;
    onCategoryChange: (category: string) => void;
    categories: readonly SelectItem[];
    rows: Contact[];
    active: SessionKey | null;
    box: ChatBoxSummary;
    listSearchInput: RefObject<HTMLInputElement>;
    accountSearchTrigger: RefObject<HTMLButtonElement>;
    onOpenAccountSearch: () => void;
    onOpen: (contact: Contact) => void;
    onHide: (contact: Contact) => void;
    listNavigation: ChatListNavigation;
}

export function ChatListPane({
    target,
    store,
    snapshot,
    ignoredGroups,
    qqMuted,
    notificationBusy,
    onMuteGroup,
    connected,
    connectionLabel,
    picker,
    onNavigate,
    listWidth,
    onListWidthChange,
    tab,
    onTabChange,
    inGroupBox,
    onGroupBoxChange,
    query,
    onQueryChange,
    unread,
    onUnreadChange,
    contactType,
    onContactTypeChange,
    category,
    onCategoryChange,
    categories,
    rows,
    active,
    box,
    listSearchInput,
    accountSearchTrigger,
    onOpenAccountSearch,
    onOpen,
    onHide,
    listNavigation,
}: ChatListPaneProps) {
    return (
        <aside className="native-chat-list-pane" aria-label="会话列表">
            <ChatDivider
                width={listWidth}
                onResize={onListWidthChange}
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
                            onGroupBoxChange(false);
                            onQueryChange('');
                        }}
                    >
                        <ArrowLeft size={16} />
                    </button>
                    <span>群消息盒子</span>
                    <span className="ml-auto text-text-tertiary text-[11px]">{box.count} 个群</span>
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
                        onQueryChange(e.target.value);
                        listNavigation.reset();
                    }}
                    onKeyDown={listNavigation.onKeyDown}
                    onBlur={listNavigation.reset}
                />
                {query && (
                    <button
                        aria-label="清除搜索"
                        onClick={() => {
                            onQueryChange('');
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
                            onUnreadChange(!unread);
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
                            onClick={() => onTabChange(value)}
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
                    onClick={onOpenAccountSearch}
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
                        onContactTypeChange(value);
                        listNavigation.reset();
                    }}
                    onCategoryChange={(value) => {
                        onCategoryChange(value);
                        listNavigation.reset();
                    }}
                />
            )}
            {tab === 'messages' && !inGroupBox && !query.trim() && (!unread || box.unread > 0) && (
                <button className="native-chat-box-entry" onClick={() => onGroupBoxChange(true)}>
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
                    active={active}
                    store={store}
                    onOpen={onOpen}
                    contacts={tab === 'contacts'}
                    listId={listNavigation.listId}
                    highlighted={listNavigation.highlighted}
                    onHide={onHide}
                    ignoredGroups={ignoredGroups}
                    qqMuted={qqMuted}
                    onIgnoreGroup={(contact, ignored) => void onMuteGroup(contact.id, ignored)}
                    notificationBusy={notificationBusy}
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
                                    onClick={() => onTabChange('contacts')}
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
                    onReconnect={() => (target.running ? store.initialize() : onNavigate('bots'))}
                />
            </div>
        </aside>
    );
}
