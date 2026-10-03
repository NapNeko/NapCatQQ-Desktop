// 搜索已加载的当前会话消息，定位后回到时间线。
import { useEffect, useId, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react';
import { Search, X } from 'lucide-react';
import type { Message } from '../../core/domain/chat/model';
import { messagePreview } from '../../core/domain/debug/segments';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import './chat-search.css';

interface ChatSearchProps {
    messages: Message[];
    onReveal: (key: string) => void;
    onClose: () => void;
    inputRef?: RefObject<HTMLInputElement>;
    history?: { loading: boolean; loaded: boolean; done: boolean; error: string };
    onLoadEarlier?: () => void;
    canLoadEarlier?: boolean;
}

const RESULTS_PER_PAGE = 50;

function highlightPreview(text: string, pattern: RegExp): ReactNode[] {
    const parts: ReactNode[] = [];
    let offset = 0;
    for (const match of text.matchAll(new RegExp(pattern.source, 'giu'))) {
        const start = match.index;
        parts.push(text.slice(offset, start));
        parts.push(<mark className="native-chat-search-highlight" key={start}>{match[0]}</mark>);
        offset = start + match[0].length;
    }
    parts.push(text.slice(offset));
    return parts;
}

export function ChatSearch({ messages, onReveal, onClose, inputRef, history, onLoadEarlier, canLoadEarlier = true }: ChatSearchProps) {
    const [query, setQuery] = useState('');
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const [limit, setLimit] = useState(RESULTS_PER_PAGE);
    const localInput = useRef<HTMLInputElement>(null);
    const resultsRef = useRef<HTMLDivElement>(null);
    const composing = useRef(false);
    const field = inputRef ?? localInput;
    const id = useId();
    const term = query.trim();
    const pattern = useMemo(() => term ? new RegExp(term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'iu') : null, [term]);
    const searchable = useMemo(() => messages.filter(message => !message.recalled).map(message => ({ message, preview: messagePreview(message.segments) })), [messages]);
    const matches = useMemo(() => pattern ? searchable.filter(item => pattern.test(item.preview)).reverse() : [], [searchable, pattern]);
    const selectedIndex = matches.findIndex(item => item.message.key === activeKey);
    const activeIndex = Math.max(0, selectedIndex);
    const active = matches[activeIndex];
    // Keep a selected message visible when a newer matching message arrives.
    const visibleLimit = Math.max(limit, activeIndex + 1);
    const visibleMatches = matches.slice(0, visibleLimit);

    useEffect(() => {
        if (selectedIndex < 0) setActiveKey(matches[0]?.message.key ?? null);
    }, [matches, selectedIndex]);
    useEffect(() => {
        if (activeKey) resultsRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' });
    }, [activeKey, activeIndex]);

    const resetQuery = (value: string) => {
        setQuery(value);
        setActiveKey(null);
        setLimit(RESULTS_PER_PAGE);
    };
    const resultsStatus = term
        ? matches.length ? `${matches.length} 条结果${visibleMatches.length < matches.length ? ` · 显示最近 ${visibleMatches.length} 条` : ''}` : `没有找到匹配消息${onLoadEarlier && !history?.done ? '，可换个关键词或加载更早消息' : '，试试其他关键词'}`
        : '输入关键词查找消息';
    const historyStatus = history?.loading
        ? history.loaded ? '正在加载更早消息…' : '正在读取消息…'
        : history?.done ? '已到最早消息' : !canLoadEarlier && onLoadEarlier ? '连接恢复后可加载更早消息' : '';

    return <section className="native-chat-message-search" aria-label="搜索当前会话的消息" onKeyDown={event => {
        if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229) return;
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose(); }
    }}>
        <div className="relative z-[1] flex shrink-0 items-center gap-2">
            <div className="native-chat-search min-w-0 flex-1">
                <Search size={14} aria-hidden />
                <input ref={field} autoFocus role="combobox" aria-label="搜索已加载消息" aria-autocomplete="list" aria-expanded={Boolean(term)} aria-controls={`${id}-results`} aria-describedby={`${id}-status ${id}-scope`} aria-activedescendant={active ? `${id}-result-${activeIndex}` : undefined} placeholder="搜索当前会话" value={query}
                    onChange={event => resetQuery(event.target.value)}
                    onCompositionStart={() => { composing.current = true; }}
                    onCompositionEnd={() => { composing.current = false; }}
                    onKeyDown={event => {
                        if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229) return;
                        if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && matches.length) {
                            event.preventDefault();
                            const nextIndex = Math.max(0, Math.min(matches.length - 1, activeIndex + (event.key === 'ArrowDown' ? 1 : -1)));
                            setLimit(current => Math.max(current, nextIndex + 1));
                            setActiveKey(matches[nextIndex].message.key);
                        } else if (event.key === 'Enter' && active) {
                            event.preventDefault();
                            onReveal(active.message.key);
                        }
                    }} />
                {query && <button type="button" aria-label="清除消息搜索" onClick={() => { resetQuery(''); field.current?.focus(); }}><X size={13} /></button>}
            </div>
            <button type="button" className="native-chat-icon" aria-label="关闭消息搜索" title="关闭搜索 · Esc" onClick={onClose}><X size={16} /></button>
        </div>
        <div className="native-chat-search-summary">
            <p id={`${id}-status`} role="status">{resultsStatus}{historyStatus && ` · ${historyStatus}`}</p>
            <p id={`${id}-scope`}>仅搜索当前会话已加载的 {searchable.length} 条消息<span className="native-chat-search-key-hint"> · ↑↓ 选择 · Enter 定位</span></p>
            {history?.error && <p role="alert" className="text-danger">读取历史失败：{history.error}</p>}
        </div>
        <div ref={resultsRef} id={`${id}-results`} role="listbox" aria-label="消息搜索结果" className="native-chat-search-results" hidden={!term}>
            {visibleMatches.map(({ message, preview }, index) => <button type="button" role="option" id={`${id}-result-${index}`} aria-selected={message.key === active?.message.key} tabIndex={-1} key={message.key}
                onMouseDown={event => event.preventDefault()} onClick={() => onReveal(message.key)}>
                <span className="mb-1 flex justify-between gap-3 text-[10px] text-text-tertiary"><span className="truncate">{message.mine ? '我' : message.senderName}</span><time className="shrink-0">{dayLabel(message.at)}</time></span>
                <span className="line-clamp-2 break-words">{pattern ? highlightPreview(preview, pattern) : preview}</span>
            </button>)}
        </div>
        {(visibleMatches.length < matches.length || (onLoadEarlier && !history?.done)) && <div className="native-chat-search-footer">
            {visibleMatches.length < matches.length && <button type="button" className="native-chat-text-button" onClick={() => setLimit(visibleLimit + RESULTS_PER_PAGE)}>显示更多结果（还有 {matches.length - visibleMatches.length} 条）</button>}
            {onLoadEarlier && !history?.done && <button type="button" className="native-chat-text-button" disabled={history?.loading || !canLoadEarlier} onClick={onLoadEarlier}>{history?.loading ? '正在加载…' : history?.error ? '重试加载更早消息' : '加载更早消息'}</button>}
        </div>}
    </section>;
}
