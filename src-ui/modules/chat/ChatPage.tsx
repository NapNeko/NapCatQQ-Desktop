// 主窗口内的原生双栏聊天。
import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowLeft, Check, ChevronDown, Info, MessagesSquare, Pin, RefreshCw, Search, Users, X } from 'lucide-react';
import { chatService } from '../../core/services/chat.service';
import { chatAccount, reconcileChatAccounts, useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { accountKey, EMPTY_DRAFT, type Contact, type Conversation, type SessionKey } from '../../core/domain/chat/model';
import { messagePreview } from '../../core/domain/debug/segments';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { isTauri } from '../../core/ipc/transport';
import { cn } from '../../shared/utils/cn';
import { ChatComposer } from './ChatComposer';
import { NativeTimeline } from './ChatTimeline';
import './chat.css';

let lastBot = '';
export function ChatPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
    const [selected, select] = useState(lastBot);
    const targets = useQuery({ queryKey: ['chat', 'targets'], queryFn: chatService.targets, refetchInterval: 15_000 });
    useEffect(() => { if (targets.data) reconcileChatAccounts(targets.data); }, [targets.data]);
    const target = targets.data?.find(t => t.bot_id === selected) ?? targets.data?.find(t => t.running) ?? targets.data?.[0];
    const picker = <div className="native-chat-identity"><span className="h-2 w-2 rounded-full bg-brand" /><select aria-label="聊天账号" value={target?.bot_id ?? ''} onChange={e => { lastBot = e.target.value; select(e.target.value); }}>{targets.data?.map(t => <option key={t.bot_id} value={t.bot_id}>{t.name} · {t.qq_id}{!t.running ? ' · 已停止' : ''}</option>)}</select><ChevronDown size={13} aria-hidden /></div>;
    return <section className="native-chat">
        <header className="native-chat-page-header"><h1 className="font-display text-[24px] font-semibold tracking-tight text-text">聊天</h1>{target && picker}{!isTauri && <span className="text-[11px] text-text-tertiary">预览</span>}</header>
        {target ? <ChatWorkspace key={accountKey(target.bot_id, String(target.qq_id))} target={target} onNavigate={onNavigate} /> : <div className="native-chat-welcome"><MessagesSquare size={36} strokeWidth={1.3} /><h2>{targets.isLoading ? '正在读取账号' : targets.isError ? '账号读取失败' : '从一个机器人开始聊天'}</h2><button className="native-chat-text-button" onClick={() => targets.isError ? void targets.refetch() : onNavigate('bots')}>{targets.isError ? '重试' : '前往机器人'}</button></div>}
    </section>;
}

function ChatWorkspace({ target, onNavigate }: { target: DebugTarget; onNavigate: (route: AppRoute) => void }) {
    const store = chatAccount(target); const snapshot = useChatSnapshot(store); const { account } = snapshot;
    const [tab, setTab] = useState<'messages' | 'contacts'>('messages');
    const [query, setQuery] = useState(''); const [unread, setUnread] = useState(false);
    const [detail, setDetail] = useState(false); const [narrowFocus, setNarrowFocus] = useState(false);
    const [messageSearch, setMessageSearch] = useState('');
    const reveal = useRef<(key: string) => void>(() => {});
    const active = account.active ? account.conversations[account.active] : undefined;
    const messages = useMemo(() => account.messages.filter(m => m.session === account.active), [account.messages, account.active]);
    useEffect(() => {
        if (target.running && target.online !== false) void store.connect().then(() => store.loadContacts());
        else void store.disconnect();
    }, [store, target.running, target.online]);
    useEffect(() => () => store.setReading(null), [store]);
    useEffect(() => { setDetail(false); setMessageSearch(''); }, [account.active]);
    const rows = useMemo(() => {
        const term = query.trim().toLocaleLowerCase();
        const source = tab === 'contacts' ? snapshot.contacts : Object.values(account.conversations);
        return source.filter(c => (!term || `${c.name} ${c.id}`.toLocaleLowerCase().includes(term)) && (tab === 'contacts' || !unread || (c as Conversation).unread > 0))
            .sort((a, b) => tab === 'contacts' ? a.type.localeCompare(b.type) || a.name.localeCompare(b.name, 'zh-CN') : Number((b as Conversation).pinned) - Number((a as Conversation).pinned) || (b as Conversation).lastAt - (a as Conversation).lastAt);
    }, [query, tab, unread, snapshot.contacts, account.conversations]);
    const connected = target.running && target.online !== false && snapshot.connection.state === 'connected';
    const connectionLabel = !target.running ? '机器人已停止' : target.online === false ? '账号未登录' : snapshot.connection.state === 'connected' ? '已连接' : snapshot.connection.state === 'connecting' ? '连接中' : snapshot.connection.state === 'reconnecting' ? '正在重连' : '连接已断开';
    const open = (contact: Contact) => { store.open(contact); setTab('messages'); setNarrowFocus(true); };
    return <div className="native-chat-workspace" data-conversation={narrowFocus}>
        <aside className="native-chat-list-pane" aria-label="会话列表">
            <label className="native-chat-search"><Search size={15} aria-hidden /><input aria-label="搜索会话或联系人" placeholder="搜索" value={query} onChange={e => setQuery(e.target.value)} />{query && <button aria-label="清除搜索" onClick={() => setQuery('')}><X size={13} /></button>}</label>
            <div className="native-chat-tabs"><div role="tablist" aria-label="聊天列表">{(['messages', 'contacts'] as const).map(value => <button key={value} role="tab" aria-selected={tab === value} onClick={() => setTab(value)}>{value === 'messages' ? '消息' : '联系人'}</button>)}</div>{tab === 'messages' ? <button className={cn('native-chat-unread', unread && 'is-active')} aria-pressed={unread} onClick={() => setUnread(!unread)}>未读</button> : <button className="native-chat-icon" aria-label="刷新联系人" disabled={snapshot.contactsLoading || !connected} onClick={() => void store.loadContacts()}><RefreshCw size={14} /></button>}</div>
            <ConversationList rows={rows} active={account.active} store={store} onOpen={open} contacts={tab === 'contacts'} />
            {!rows.length && <div className="native-chat-list-empty">{snapshot.contactsLoading ? '正在读取联系人…' : query ? '没有找到匹配项' : unread ? '没有未读消息' : tab === 'contacts' ? '暂无联系人' : <><span>还没有会话</span><button className="native-chat-text-button" onClick={() => setTab('contacts')}>从联系人开始</button></>}</div>}
            <div className="native-chat-connection"><span className={cn('h-1.5 w-1.5 rounded-full', connected ? 'bg-success' : 'bg-text-disabled')} /><span>{connectionLabel}</span>{!connected && <button aria-label="重新连接聊天" className="native-chat-icon ml-auto" onClick={() => target.running ? void store.connect().then(() => store.loadContacts()) : onNavigate('bots')}><RefreshCw size={13} /></button>}</div>
        </aside>
        <main className="native-chat-message-pane" aria-label="当前会话">
            {snapshot.error && <div role="status" className="native-chat-error">{snapshot.error}<button onClick={() => void store.connect().then(() => store.loadContacts())}>重试</button></div>}
            {active ? <>
                <header className="native-chat-conversation-header"><button className="native-chat-back native-chat-icon" aria-label="返回会话列表" onClick={() => { setNarrowFocus(false); store.setReading(null); }}><ArrowLeft size={18} /></button><Avatar contact={active} small /><div className="min-w-0 flex-1"><h2 className="truncate text-[14px] font-semibold">{active.name}</h2><p className="text-[11px] text-text-tertiary">{active.type === 'group' ? active.members ? `${active.members} 位成员` : '群聊' : active.id}</p></div><button className="native-chat-icon" aria-label="搜索当前会话" onClick={() => { setDetail(true); setMessageSearch(''); }}><Search size={17} /></button><button className="native-chat-icon" aria-label="会话资料" aria-expanded={detail} onClick={() => setDetail(!detail)}><Info size={17} /></button></header>
                <NativeTimeline key={`timeline:${active.key}`} store={store} contact={active} messages={messages} revealRef={reveal} visible={narrowFocus} />
                <ChatComposer key={`composer:${active.key}`} store={store} contact={active} disabledReason={connected ? '' : connectionLabel} />
                {detail && <aside className="native-chat-details" aria-label="会话资料"><div className="flex items-center justify-between"><h3 className="text-[14px] font-semibold">会话资料</h3><button className="native-chat-icon" aria-label="关闭会话资料" onClick={() => setDetail(false)}><X size={17} /></button></div><div className="flex flex-col items-center gap-3 py-6"><Avatar contact={active} /><strong className="text-[14px]">{active.name}</strong><span className="text-[12px] text-text-tertiary">{active.type === 'group' ? '群号' : 'QQ'} {active.id}</span></div><button className="native-chat-detail-action" aria-pressed={active.pinned} onClick={() => store.pin(active.key)}><Pin size={15} />置顶会话{active.pinned && <Check size={15} className="ml-auto text-brand" />}</button><label className="native-chat-search mt-5"><Search size={14} /><input autoFocus aria-label="搜索已加载消息" placeholder="搜索已加载消息" value={messageSearch} onChange={e => setMessageSearch(e.target.value)} /></label>{messageSearch && <div className="native-chat-search-results">{messages.filter(m => messagePreview(m.segments).toLocaleLowerCase().includes(messageSearch.toLocaleLowerCase())).slice(-50).map(m => <button key={m.key} onClick={() => { setDetail(false); reveal.current(m.key); }}><span className="block text-[11px] text-text-tertiary">{m.senderName}</span><span className="line-clamp-2">{messagePreview(m.segments)}</span></button>)}</div>}<p className="mt-5 text-[11px] leading-relaxed text-text-tertiary">消息和草稿仅保留在本次应用会话中，最多缓存 5,000 条消息。</p></aside>}
            </> : <div className="native-chat-welcome"><span className="native-chat-welcome-mark"><MessagesSquare size={36} strokeWidth={1.25} /></span><h2>开始一段对话</h2><p>选择会话，或从联系人发起聊天</p><button className="native-chat-text-button" onClick={() => setTab('contacts')}>查看联系人</button></div>}
        </main>
    </div>;
}

export function Avatar({ contact, small = false }: { contact: Pick<Contact, 'name' | 'type'>; small?: boolean }) {
    return <span aria-hidden className={cn('native-chat-avatar', small && 'is-small', contact.type === 'group' && 'is-group')}>{contact.type === 'group' ? <Users size={small ? 16 : 19} strokeWidth={1.65} /> : contact.name.slice(0, 1)}</span>;
}
function ConversationList({ rows, active, store, onOpen, contacts }: { rows: Contact[]; active: SessionKey | null; store: ChatAccountStore; onOpen: (c: Contact) => void; contacts: boolean }) {
    const scroll = useRef<HTMLDivElement>(null); const account = useChatSnapshot(store).account;
    const virtual = useVirtualizer({ count: rows.length, getScrollElement: () => scroll.current, estimateSize: () => 68, overscan: 6, getItemKey: i => rows[i].key });
    return <div className="native-chat-conversations" ref={scroll}><div style={{ height: virtual.getTotalSize(), position: 'relative' }}>{virtual.getVirtualItems().map(row => {
        const contact = rows[row.index]; const conversation = account.conversations[contact.key]; const draft = account.drafts[contact.key] ?? EMPTY_DRAFT;
        return <div className="native-chat-conversation-row" key={row.key} style={{ transform: `translateY(${row.start}px)` }} data-active={active === contact.key}><button className="native-chat-conversation-button" onClick={() => onOpen(contact)} aria-current={active === contact.key ? 'true' : undefined}><Avatar contact={contact} /><span className="min-w-0 flex-1"><span className="flex items-center gap-1.5"><span className="truncate text-[13px] font-medium">{contact.name}</span>{conversation?.pinned && <Pin size={10} className="shrink-0 text-text-tertiary" />}</span><span className="mt-1 block truncate text-[11.5px] text-text-tertiary">{contacts ? `${contact.type === 'group' ? '群聊' : '好友'} · ${contact.id}` : draft.text || draft.attachments.length ? <><span className="text-brand">草稿 </span>{draft.text || '[附件]'}</> : conversation?.preview || '暂无消息'}</span></span><span className="flex h-10 shrink-0 flex-col items-end justify-between"><time className="text-[10px] text-text-tertiary">{!contacts && conversation?.lastAt ? new Date(conversation.lastAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' }) : ''}</time>{!contacts && (conversation?.unread ?? 0) > 0 && <span className="native-chat-badge">{conversation.unread > 99 ? '99+' : conversation.unread}</span>}</span></button></div>;
    })}</div></div>;
}
