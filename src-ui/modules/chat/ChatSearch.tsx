// 搜索已加载的当前会话消息，定位后回到时间线。
import { useMemo, useRef, useState, type RefObject } from 'react';
import { Search, X } from 'lucide-react';
import type { Message } from '../../core/domain/chat/model';
import { messagePreview } from '../../core/domain/debug/segments';
import { dayLabel } from '../../core/domain/debug/chatFormat';

export function ChatSearch({ messages, onReveal, onClose, inputRef }: { messages: Message[]; onReveal: (key: string) => void; onClose: () => void; inputRef?: RefObject<HTMLInputElement> }) {
    const [query, setQuery] = useState('');
    const localInput = useRef<HTMLInputElement>(null);
    const field = inputRef ?? localInput;
    const term = query.trim().toLocaleLowerCase();
    const matches = useMemo(() => term ? messages.filter(m => !m.recalled && messagePreview(m.segments).toLocaleLowerCase().includes(term)).reverse() : [], [messages, term]);
    return <section className="native-chat-message-search" aria-label="搜索当前会话的消息" onKeyDown={e => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); } }}>
        <div className="relative z-[1] flex shrink-0 items-center gap-2"><div className="native-chat-search min-w-0 flex-1"><Search size={14} aria-hidden /><input ref={field} autoFocus aria-label="搜索已加载消息" placeholder="搜索已加载消息" value={query} onChange={e => setQuery(e.target.value)} />{query && <button type="button" aria-label="清除消息搜索" onClick={() => { setQuery(''); field.current?.focus(); }}><X size={13} /></button>}</div><button type="button" className="native-chat-icon" aria-label="关闭消息搜索" title="关闭搜索 · Esc" onClick={onClose}><X size={16} /></button></div>
        {term && <><p role="status" className="px-1 pt-2 text-[10px] text-text-tertiary">{matches.length ? `${matches.length} 条结果${matches.length > 50 ? ' · 显示最近 50 条' : ''}` : '没有找到匹配消息'}</p><div className="native-chat-search-results">{matches.slice(0, 50).map(message => <button key={message.key} onClick={() => onReveal(message.key)}><span className="mb-1 flex justify-between gap-3 text-[10px] text-text-tertiary"><span className="truncate">{message.mine ? '我' : message.senderName}</span><time className="shrink-0">{dayLabel(message.at)}</time></span><span className="line-clamp-2 break-words">{messagePreview(message.segments)}</span></button>)}</div></>}
    </section>;
}
