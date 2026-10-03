// 虚拟时间线复用消息段渲染和贴底逻辑，样式与阅读状态独立。
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MutableRefObject } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown, Copy, Reply as ReplyIcon, RotateCcw } from 'lucide-react';
import { SegmentList, isMediaOnly, isPictureOnly } from '../debug/right/SegmentView';
import { ChatViewContext, useChatView } from '../debug/right/chatContext';
import { useStickToBottom } from '../debug/right/useStickToBottom';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { EMPTY_DRAFT, accountKey, type Contact, type Message } from '../../core/domain/chat/model';
import { messagePreview } from '../../core/domain/debug/segments';
import { chatService } from '../../core/services/chat.service';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { ChatImageViewer } from './ChatImageViewer';
import { chatMediaService } from '../../core/services/chat-media.service';
import { ChatAvatar as Avatar } from './ChatAvatar';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { cn } from '../../shared/utils/cn';
import { useHistoryPaging } from './useHistoryPaging';

export function NativeTimeline({ store, contact, messages, revealRef, visible }: { store: ChatAccountStore; contact: Contact; messages: Message[]; revealRef: MutableRefObject<(key: string) => void>; visible: boolean }) {
    const scroll = useRef<HTMLDivElement>(null); const latest = useRef(messages); latest.current = messages;
    const historyAnchor = useRef<{ height: number; top: number } | null>(null);
    const [image, showImage] = useState(''); const [error, setError] = useState(''); const [highlight, setHighlight] = useState('');
    const snapshot = useChatSnapshot(store); const history = snapshot.history[contact.key];
    useEffect(() => { void store.ensureHistory(contact.key); }, [store, contact.key, snapshot.connection.state, history]);
    const motion = useMotion();
    const virtual = useVirtualizer({ count: messages.length, getScrollElement: () => scroll.current, estimateSize: () => 108, getItemKey: i => latest.current[i]?.key ?? i, overscan: 7, paddingStart: 12, paddingEnd: 24, anchorTo: 'end' });
    const key = `${accountKey(store.target.bot_id, String(store.target.qq_id))}/${contact.key}`;
    const stick = useStickToBottom({ scrollRef: scroll, virtualizer: virtual, items: messages, memoryKey: `native-chat:${key}`, resetToken: key, filterToken: '', animate: false });
    const paging = useHistoryPaging({ scroll, enabled: snapshot.connection.state === 'connected' && !!history?.loaded && !history.loading && !history.done && !history.error, load: () => store.history(contact.key), detach: stick.detach });
    useEffect(() => {
        if (history?.loading && history.loaded && !historyAnchor.current && scroll.current) {
            historyAnchor.current = { height: scroll.current.scrollHeight, top: scroll.current.scrollTop };
        }
        if (!history?.loaded) historyAnchor.current = null;
    }, [history?.loading, history?.loaded]);
    useLayoutEffect(() => {
        const anchor = historyAnchor.current;
        if (!anchor || history?.loading || !history.loaded || !scroll.current) return;
        const element = scroll.current;
        element.scrollTop = anchor.top + (element.scrollHeight - anchor.height);
        historyAnchor.current = null;
    }, [history?.loading, history?.loaded, messages.length]);
    useEffect(() => {
        const update = () => store.setReading(!stick.away && document.visibilityState === 'visible' && (scroll.current?.clientWidth ?? 0) > 0 ? contact.key : null);
        update(); document.addEventListener('visibilitychange', update);
        const observer = new ResizeObserver(update); if (scroll.current) observer.observe(scroll.current);
        return () => { document.removeEventListener('visibilitychange', update); observer.disconnect(); store.setReading(null); };
    }, [store, contact.key, stick.away, visible]);
    useEffect(() => {
        revealRef.current = messageKey => { const index = latest.current.findIndex(m => m.key === messageKey); if (index < 0) return; stick.detach(); virtual.scrollToIndex(index, { align: 'center' }); setHighlight(messageKey); };
        return () => { revealRef.current = () => {}; };
    }, [revealRef, virtual, stick.detach]);
    useEffect(() => { if (!highlight) return; const timer = setTimeout(() => setHighlight(''), 1600); return () => clearTimeout(timer); }, [highlight]);
    const fallback = useChatView();
    const view = useMemo(() => ({
        ...fallback,
        openImage: showImage,
        readImage: (data: Record<string, unknown>, refresh?: boolean) => chatMediaService.image(store.target, data, refresh),
        readForward: (data: Record<string, unknown>) => chatMediaService.forward(store.target, data),
        readRecord: (data: Record<string, unknown>) => chatMediaService.record(store.target, data),
        readVideo: (data: Record<string, unknown>, refresh?: boolean) => chatMediaService.video(store.target, data, refresh),
        readRecordText: (messageId: string) => chatMediaService.transcript(store.target, messageId),
        openLink: (url: string) => { void chatService.openLink(url).catch(e => setError(String(e))); },
        nameOf: (userId: number) => snapshot.account.messages.find(m => m.senderId === String(userId))?.senderName,
        findMessage: (messageId: number) => {
            const m = latest.current.find(m => m.id === String(messageId));
            return m ? { kind: 'message' as const, key: m.key, seq: 0, at: m.at, session: m.session, direction: m.mine ? 'out' as const : 'in' as const, senderId: Number(m.senderId), senderName: m.senderName, messageId, segments: m.segments, raw: {} } : undefined;
        },
        revealMessage: (messageId: number) => { const m = latest.current.find(m => m.id === String(messageId)); if (!m) return false; revealRef.current(m.key); return true; },
    }), [fallback, snapshot.account.messages, revealRef, store]);
    const reply = (message: Message) => { if (message.id) store.draft(contact.key, { ...(store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT), reply: { id: message.id, name: message.mine ? '我' : message.senderName, preview: messagePreview(message.segments) } }); };
    return <ChatViewContext.Provider value={view}>
        <div className="native-chat-timeline-wrap">
            <div className="native-chat-history-hotzone">
                <div className="native-chat-history" data-active={!!history?.loading || !!history?.error}><button disabled={history?.loading || history?.done || snapshot.connection.state !== 'connected'} onClick={() => void store.history(contact.key)}>{history?.loading ? '正在加载…' : history?.error ? '重试读取历史' : history?.done ? '已到最早消息' : '加载更早消息'}</button>{snapshot.account.gap && <span>部分消息未接收，可尝试加载历史</span>}{history?.error && <span role="status" className="text-danger">{history.error}</span>}</div>
            </div>
            <div ref={scroll} className="native-chat-timeline" tabIndex={0} aria-label="消息记录"
            onScroll={() => { stick.handlers.onScroll(); paging.onScroll(); }}
            onWheel={e => { stick.handlers.onWheel(e); paging.onWheel(e); }}
            onKeyDown={e => { stick.handlers.onKeyDown(e); paging.onKeyDown(e); }}
            onPointerDown={e => { stick.handlers.onPointerDown(e); if (e.target === e.currentTarget) paging.onPointerDown(); }}
            onTouchStart={e => { stick.handlers.onTouchStart(e); paging.onTouchStart(e); }}
            onTouchMove={e => { stick.handlers.onTouchMove(e); paging.onTouchMove(e); }}>
            {!messages.length && <div className="native-chat-message-empty">{history?.loading ? '正在读取消息' : '还没有消息，从一句问候开始'}</div>}
            <div style={{ height: virtual.getTotalSize(), position: 'relative', width: '100%' }}>{virtual.getVirtualItems().map(row => {
                const message = messages[row.index]; const previous = messages[row.index - 1];
                const showTime = !previous || message.at - previous.at > 5 * 60_000 || new Date(message.at).toDateString() !== new Date(previous.at).toDateString();
                const continuation = !showTime && previous?.senderId === message.senderId && previous?.mine === message.mine && message.at - previous.at < 3 * 60_000;
                return <div key={row.key} data-index={row.index} ref={virtual.measureElement} style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${row.start}px)` }}>
                    {showTime && <div className="native-chat-time">{dayLabel(message.at)}</div>}
                    <article className={cn('native-chat-message', message.mine && 'is-mine', continuation && 'is-continuation')} data-highlight={highlight === message.key}>
                        <div className={continuation ? 'invisible' : ''}><Avatar contact={{ type: 'private', id: message.senderId, name: message.mine ? store.target.name : message.senderName }} small /></div>
                        <div className="native-chat-message-body">{!message.mine && !continuation && <div className="native-chat-sender">{message.senderName}</div>}
                            <div className={cn('native-chat-bubble', isMediaOnly(message.segments) && 'is-media-only', isPictureOnly(message.segments) && 'is-picture-only')}>{message.recalled ? <span className="text-text-tertiary">消息已撤回</span> : <SegmentList segments={message.segments} mine={message.mine} messageId={message.id} />}</div>
                            {message.mine && message.status !== 'sent' && <div className={cn('native-chat-send-status', message.status !== 'sending' && 'text-danger')} title={message.error}>{message.status === 'sending' ? '发送中…' : message.status === 'unknown' ? '发送结果未确认，请核实后再发送' : '发送失败'}{message.error && <span className="block">{message.error}</span>}</div>}
                        </div>
                        {!message.recalled && <div className="native-chat-message-actions">
                            {message.id && <button className="native-chat-icon" aria-label={`回复${message.senderName}的消息`} title="回复" onClick={() => reply(message)}><ReplyIcon size={14} /></button>}
                            <button className="native-chat-icon" aria-label="复制消息" title="复制" onClick={() => void navigator.clipboard.writeText(messagePreview(message.segments)).catch(() => setError('复制失败'))}><Copy size={13} /></button>
                            {message.status === 'failed' && <button className="native-chat-icon" aria-label="将失败消息放回输入框" title="放回输入框" onClick={() => {
                                const draft = store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
                                store.draft(contact.key, recoverDraft(message, draft));
                            }}><RotateCcw size={13} /></button>}
                        </div>}
                    </article>
                </div>;
            })}</div>
        </div>{(stick.away || stick.unseen > 0) && <button className="native-chat-latest" onClick={() => stick.jumpToLatest(motion.enabled)}><ArrowDown size={14} />{stick.unseen > 0 ? `${stick.unseen} 条新消息` : '回到最新'}</button>}</div>
        {error && <button role="status" className="native-chat-error" onClick={() => setError('')}>{error}</button>}
        <ChatImageViewer src={image} onClose={() => showImage('')} />
    </ChatViewContext.Provider>;
}
