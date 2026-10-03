// 主窗口内的原生双栏聊天。
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ArrowLeft, ChevronRight, Inbox, MessagesSquare, RefreshCw, Search, X } from 'lucide-react';
import { chatService } from '../../core/services/chat.service';
import { chatAccount, reconcileChatAccounts, useChatSnapshot } from '../../hooks/chat/chatStore';
import { accountKey, type Contact } from '../../core/domain/chat/model';
import { conversationRows, groupBoxSummary } from '../../core/domain/chat/groupBox';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { cn } from '../../shared/utils/cn';
import { ChatComposer } from './ChatComposer';
import { NativeTimeline } from './ChatTimeline';
import { BotPicker } from '../debug/BotPicker';
import { ChatAvatar as Avatar } from './ChatAvatar';
import { ChatDetails } from './ChatDetails';
import { ChatSearch } from './ChatSearch';
import { ChatDivider } from './ChatDivider';
import { ConversationList, useConversationNavigation } from './ConversationList';
import { setChatPreferences, setConversationHidden, useChatPreferences } from './chatPreferences';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui/Dialog';
import { useMotion } from '../../hooks/preferences/useMotion';
import './chat.css';

let lastBot = '';
export function ChatPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
    const [selected, select] = useState(lastBot);
    const targets = useQuery({ queryKey: ['chat', 'targets'], queryFn: chatService.targets, refetchInterval: 15_000 });
    useEffect(() => { if (targets.data) reconcileChatAccounts(targets.data); }, [targets.data]);
    const target = targets.data?.find(t => t.bot_id === selected) ?? targets.data?.find(t => t.running) ?? targets.data?.[0];
    const picker = <BotPicker compact ariaLabel="聊天账号" targets={targets.data ?? []} selected={target ?? null} loading={targets.isLoading} onSelect={botId => { lastBot = botId; select(botId); }} onManageBots={() => onNavigate('bots')} />;
    return <section className="native-chat">
        {target ? <ChatWorkspace key={accountKey(target.bot_id, String(target.qq_id))} target={target} picker={picker} onNavigate={onNavigate} /> : <div className="native-chat-welcome"><MessagesSquare size={36} strokeWidth={1.3} /><h2>{targets.isLoading ? '正在读取账号' : targets.isError ? '账号读取失败' : '从一个机器人开始聊天'}</h2><button className="native-chat-text-button" onClick={() => targets.isError ? void targets.refetch() : onNavigate('bots')}>{targets.isError ? '重试' : '前往机器人'}</button></div>}
    </section>;
}

function ChatWorkspace({ target, picker, onNavigate }: { target: DebugTarget; picker: ReactNode; onNavigate: (route: AppRoute) => void }) {
    const store = chatAccount(target); const snapshot = useChatSnapshot(store); const { account } = snapshot;
    const preferences = useChatPreferences();
    const motion = useMotion();
    const identity = accountKey(target.bot_id, String(target.qq_id));
    const hidden = useMemo(() => new Set(preferences.hiddenConversations[identity] ?? []), [preferences.hiddenConversations, identity]);
    const [listWidth, setListWidth] = useState(preferences.listWidth);
    const [tab, setTab] = useState<'messages' | 'contacts'>('messages');
    const [inGroupBox, setInGroupBox] = useState(false);
    const [query, setQuery] = useState(''); const [unread, setUnread] = useState(false);
    const [searchOpen, setSearchOpen] = useState(false); const [narrowFocus, setNarrowFocus] = useState(false);
    const [detailsOpen, setDetailsOpen] = useState(false);
    const searchTrigger = useRef<HTMLButtonElement>(null);
    const searchInput = useRef<HTMLInputElement>(null);
    const listSearchInput = useRef<HTMLInputElement>(null);
    const composerInput = useRef<HTMLTextAreaElement>(null);
    const reveal = useRef<(key: string) => void>(() => {});
    const active = account.active ? account.conversations[account.active] : undefined;
    const messages = useMemo(() => account.messages.filter(m => m.session === account.active), [account.messages, account.active]);
    useEffect(() => {
        void store.initialize();
    }, [store, target.running, target.online]);
    useEffect(() => () => store.setReading(null), [store]);
    useEffect(() => { setSearchOpen(false); setDetailsOpen(false); }, [account.active]);
    useEffect(() => { if (searchOpen) searchInput.current?.focus(); }, [searchOpen]);
    const rows = useMemo(() => {
        const term = query.trim().toLocaleLowerCase();
        if (tab === 'messages') return conversationRows(Object.values(account.conversations).filter(c => !hidden.has(c.key)), { box: inGroupBox, unread, query });
        return snapshot.contacts.filter(c => !term || `${c.name} ${c.id}`.toLocaleLowerCase().includes(term)).sort((a, b) => a.type.localeCompare(b.type) || a.name.localeCompare(b.name, 'zh-CN'));
    }, [query, tab, unread, inGroupBox, snapshot.contacts, account.conversations, hidden]);
    const box = useMemo(() => groupBoxSummary(Object.values(account.conversations).filter(c => !hidden.has(c.key))), [account.conversations, hidden]);
    const connected = target.running && target.online !== false && snapshot.connection.state === 'connected';
    const connectionLabel = !target.running ? '机器人已停止' : target.online === false ? '账号未登录' : snapshot.connection.state === 'connected' ? '已连接' : snapshot.connection.state === 'connecting' ? '连接中' : snapshot.connection.state === 'reconnecting' ? '正在重连' : '连接已断开';
    const open = (contact: Contact) => { setConversationHidden(identity, contact.key, false); store.open(contact); setInGroupBox(contact.type === 'group'); setTab('messages'); setNarrowFocus(true); };
    const hide = (contact: Contact) => { setConversationHidden(identity, contact.key, true); store.close(contact.key); setNarrowFocus(false); };
    const focusComposer = () => requestAnimationFrame(() => { const input = composerInput.current; if (input?.isConnected) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); } });
    const listNavigation = useConversationNavigation(rows, contact => { open(contact); focusComposer(); });
    const closeSearch = () => { setSearchOpen(false); searchTrigger.current?.focus(); };
    const openSearch = () => { setSearchOpen(true); setNarrowFocus(true); searchInput.current?.focus(); };
    return <div className="native-chat-workspace" style={{ '--chat-list-width': `${listWidth}px` } as CSSProperties} data-conversation={narrowFocus} data-motion={motion.enabled} onKeyDown={e => {
        if (e.defaultPrevented || e.nativeEvent.isComposing || e.keyCode === 229 || e.altKey) return;
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); setNarrowFocus(false); setInGroupBox(false); store.setReading(null); requestAnimationFrame(() => { const input = listSearchInput.current; if (input?.isConnected) { input.focus(); input.select(); } }); }
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f' && active) { e.preventDefault(); openSearch(); }
    }}>
        <aside className="native-chat-list-pane" aria-label="会话列表">
            <ChatDivider width={listWidth} onResize={setListWidth} onCommit={width => setChatPreferences({ listWidth: width })} />
            <div className="native-chat-box-heading-wrap" data-open={inGroupBox && tab === 'messages'} aria-hidden={!(inGroupBox && tab === 'messages')} {...(!(inGroupBox && tab === 'messages') ? { inert: '' } : {})}>
                <div className="native-chat-box-heading"><button className="native-chat-icon" aria-label="返回全部会话" onClick={() => { setInGroupBox(false); setQuery(''); }}><ArrowLeft size={16} /></button><span>群消息盒子</span><span className="ml-auto text-text-tertiary text-[11px]">{box.count} 个群</span></div>
            </div>
            <label className="native-chat-search"><Search size={15} aria-hidden /><input ref={listSearchInput} role="combobox" aria-label="搜索会话或联系人" aria-autocomplete="list" aria-expanded={rows.length > 0} aria-controls={rows.length ? listNavigation.listId : undefined} aria-activedescendant={listNavigation.activeDescendant} aria-keyshortcuts="Control+k Meta+k" title="搜索会话或联系人 · Ctrl+K / ⌘K" placeholder="搜索" value={query} onChange={e => { setQuery(e.target.value); listNavigation.reset(); }} onKeyDown={listNavigation.onKeyDown} onBlur={listNavigation.reset} />{query && <button aria-label="清除搜索" onClick={() => { setQuery(''); listNavigation.reset(); listSearchInput.current?.focus(); }}><X size={13} /></button>}</label>
            <div className="native-chat-tabs"><div role="tablist" aria-label="聊天列表">{(['messages', 'contacts'] as const).map(value => <button key={value} role="tab" aria-selected={tab === value} onClick={() => { setTab(value); }}>{value === 'messages' ? '消息' : '联系人'}</button>)}</div>{tab === 'messages' ? <button className={cn('native-chat-unread', unread && 'is-active')} aria-pressed={unread} onClick={() => setUnread(!unread)}>未读</button> : <button className="native-chat-icon" aria-label="刷新联系人" disabled={snapshot.contactsLoading || !connected} onClick={() => void store.loadContacts()}><RefreshCw size={14} /></button>}</div>
            {tab === 'messages' && !inGroupBox && !query.trim() && (!unread || box.unread > 0) && <button className="native-chat-box-entry" onClick={() => setInGroupBox(true)}><span className="native-chat-avatar is-group"><Inbox size={20} /></span><span className="min-w-0 flex-1 text-left"><span className="text-[13px] font-medium">群消息盒子</span><span className="mt-1 block truncate text-[11px] text-text-tertiary">{box.latest ? `${box.latest.name}：${box.latest.preview || '暂无消息'}` : '暂无群消息'}</span></span>{box.unread > 0 && <span className="native-chat-badge">{box.unread > 99 ? '99+' : box.unread}</span>}<ChevronRight size={13} className="text-text-tertiary" /></button>}
            {rows.length > 0 && <ConversationList key={`${tab}:${inGroupBox}`} rows={rows} active={account.active} store={store} onOpen={open} contacts={tab === 'contacts'} listId={listNavigation.listId} highlighted={listNavigation.highlighted} onHide={hide} />}
            {!rows.length && <div className="native-chat-list-empty">{!snapshot.hydrated && !snapshot.archiveError ? '正在恢复聊天记录…' : snapshot.recentLoading ? '正在同步最近会话…' : snapshot.contactsLoading ? '正在读取联系人…' : query ? '没有找到匹配项' : unread ? '没有未读消息' : inGroupBox && tab === 'messages' ? '暂无群消息' : tab === 'contacts' ? '暂无联系人' : <><span>{box.count ? '群会话已收进消息盒子' : '还没有会话'}</span><button className="native-chat-text-button" onClick={() => setTab('contacts')}>从联系人开始</button></>}</div>}
            <div className="native-chat-connection">
                <div className="native-chat-account-picker">{picker}</div>
                <div className="native-chat-connection-state"><span className={cn('h-1.5 w-1.5 rounded-full', connected ? 'bg-success' : 'bg-text-disabled')} /><span>{connectionLabel}</span>{!connected && <button aria-label="重新连接聊天" className="native-chat-icon ml-auto" onClick={() => target.running ? void store.initialize() : onNavigate('bots')}><RefreshCw size={13} /></button>}</div>
            </div>
            {snapshot.archiveError && <div role="status" className="native-chat-error">{snapshot.archiveError}<button onClick={() => void (snapshot.hydrated ? store.flushArchive() : store.restore())}>重试</button></div>}
            {snapshot.recentError && <div role="status" className="native-chat-error">{snapshot.recentError}<button onClick={() => void store.loadRecent(true)}>重试</button></div>}
        </aside>
        <main className="native-chat-message-pane" aria-label="当前会话">
            {snapshot.error && <div role="status" className="native-chat-error">{snapshot.error}<button onClick={() => void store.initialize()}>重试</button></div>}
            {active ? <>
                <header className="native-chat-conversation-header"><button className="native-chat-back native-chat-icon" aria-label="返回会话列表" onClick={() => { setNarrowFocus(false); store.setReading(null); }}><ArrowLeft size={18} /></button><button type="button" className="native-chat-profile" aria-label={`查看${active.name}的资料`} aria-expanded={detailsOpen} onClick={() => setDetailsOpen(true)}><Avatar contact={active} small /><span className="min-w-0 text-left"><span className="block truncate text-[14px] font-semibold">{active.name}</span><span className="block text-[11px] text-text-tertiary">{active.type === 'group' ? active.members ? `${active.members} 位成员` : '群聊' : active.id}</span></span></button><button ref={searchTrigger} className="native-chat-icon" aria-label="搜索当前会话" title="搜索消息 · Ctrl+F" aria-expanded={searchOpen} onClick={() => setSearchOpen(!searchOpen)}><Search size={17} /></button><ChatDetails key={active.key} contact={active} target={target} open={detailsOpen} onOpenChange={setDetailsOpen} onPin={() => store.pin(active.key)} onMessage={open} onSearch={openSearch} /></header>
                <Dialog open={searchOpen} onOpenChange={setSearchOpen}><DialogContent size="lg" hideClose className="native-chat-search-dialog" onCloseAutoFocus={event => { event.preventDefault(); searchTrigger.current?.focus(); }}>
                    <DialogTitle className="native-chat-search-title"><span>聊天记录</span>{active.name}</DialogTitle>
                    <ChatSearch inputRef={searchInput} messages={messages} history={snapshot.history[active.key]} canLoadEarlier={connected} onLoadEarlier={() => void store.history(active.key)} onClose={closeSearch} onReveal={key => { closeSearch(); requestAnimationFrame(() => reveal.current(key)); }} />
                </DialogContent></Dialog>
                <NativeTimeline key={`timeline:${active.key}`} store={store} contact={active} messages={messages} revealRef={reveal} visible={narrowFocus} onFocusComposer={focusComposer} />
                <ChatComposer key={`composer:${active.key}`} store={store} contact={active} disabledReason={connected ? '' : connectionLabel} sendShortcut={preferences.sendShortcut} onSendShortcutChange={sendShortcut => setChatPreferences({ sendShortcut })} inputRef={composerInput} />
            </> : <div className="native-chat-welcome"><span className="native-chat-welcome-mark"><MessagesSquare size={36} strokeWidth={1.25} /></span><h2>开始一段对话</h2><p>选择会话，或从联系人发起聊天</p><button className="native-chat-text-button" onClick={() => setTab('contacts')}>查看联系人</button></div>}
        </main>
    </div>;
}
