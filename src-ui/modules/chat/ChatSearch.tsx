// 搜索已加载的当前会话消息，定位后回到时间线。
import {
    Fragment,
    useEffect,
    useId,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type RefObject,
} from 'react';
import { Filter, MapPin, Search, X } from 'lucide-react';
import type { Message } from '../../core/domain/chat/model';
import { messagePreview } from '../../core/domain/debug/segments';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { ChatAvatar } from './ChatAvatar';
import { Popover, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
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
const categories = [
    ['all', '全部'],
    ['media', '图片/视频'],
    ['emoji', '表情'],
    ['file', '文件'],
    ['link', '链接'],
] as const;
type Category = (typeof categories)[number][0];
function inCategory(message: Message, category: Category) {
    if (category === 'all') return true;
    return message.segments.some((segment) => {
        const sticker =
            segment.type === 'mface' ||
            segment.type === 'face' ||
            (segment.type === 'image' &&
                Number(segment.data.sub_type ?? segment.data.subType) === 1);
        if (category === 'emoji') return sticker;
        if (category === 'media')
            return segment.type === 'video' || (segment.type === 'image' && !sticker);
        if (category === 'file') return segment.type === 'file';
        return (
            ['text', 'markdown', 'json', 'xml'].includes(segment.type) &&
            Object.values(segment.data).some(
                (value) => typeof value === 'string' && /https?:\/\//i.test(value),
            )
        );
    });
}
function dateGroup(at: number) {
    const date = new Date(at);
    return `${date.getFullYear()}/${String(date.getMonth() + 1).padStart(2, '0')}/${String(date.getDate()).padStart(2, '0')}`;
}

function SearchPicture({ message }: { message: Message }) {
    const picture = message.segments.find((segment) => ['image', 'mface'].includes(segment.type));
    const url =
        picture &&
        [picture.data.url, picture.data.file].find(
            (value) =>
                typeof value === 'string' && /^(https?:\/\/|data:image\/|blob:)/i.test(value),
        );
    const [failed, setFailed] = useState('');
    if (typeof url !== 'string' || failed === url) return null;
    return (
        <span className="native-chat-search-picture">
            <img
                src={url}
                alt="图片预览"
                loading="lazy"
                decoding="async"
                referrerPolicy="no-referrer"
                onError={() => setFailed(url)}
            />
        </span>
    );
}

function highlightPreview(text: string, pattern: RegExp): ReactNode[] {
    const parts: ReactNode[] = [];
    let offset = 0;
    for (const match of text.matchAll(new RegExp(pattern.source, 'giu'))) {
        const start = match.index;
        parts.push(text.slice(offset, start));
        parts.push(
            <mark className="native-chat-search-highlight" key={start}>
                {match[0]}
            </mark>,
        );
        offset = start + match[0].length;
    }
    parts.push(text.slice(offset));
    return parts;
}

export function ChatSearch({
    messages,
    onReveal,
    onClose,
    inputRef,
    history,
    onLoadEarlier,
    canLoadEarlier = true,
}: ChatSearchProps) {
    const [query, setQuery] = useState('');
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const [limit, setLimit] = useState(RESULTS_PER_PAGE);
    const [category, setCategory] = useState<Category>('all');
    const [sender, setSender] = useState('');
    const [from, setFrom] = useState('');
    const [to, setTo] = useState('');
    const localInput = useRef<HTMLInputElement>(null);
    const resultsRef = useRef<HTMLDivElement>(null);
    const composing = useRef(false);
    const field = inputRef ?? localInput;
    const id = useId();
    const term = query.trim();
    const pattern = useMemo(
        () => (term ? new RegExp(term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'iu') : null),
        [term],
    );
    const searchable = useMemo(
        () =>
            messages
                .filter((message) => !message.recalled)
                .map((message) => ({ message, preview: messagePreview(message.segments) })),
        [messages],
    );
    const senders = useMemo(
        () => [
            ...new Map(
                searchable.map(({ message }) => [
                    message.senderId,
                    message.mine ? '我' : message.senderName,
                ]),
            ).entries(),
        ],
        [searchable],
    );
    const matches = useMemo(
        () =>
            searchable
                .filter(
                    ({ message, preview }) =>
                        (!pattern || pattern.test(preview)) &&
                        inCategory(message, category) &&
                        (!sender || message.senderId === sender) &&
                        (!from || message.at >= new Date(`${from}T00:00:00`).getTime()) &&
                        (!to || message.at < new Date(`${to}T23:59:59.999`).getTime() + 1),
                )
                .reverse(),
        [searchable, pattern, category, sender, from, to],
    );
    const selectedIndex = matches.findIndex((item) => item.message.key === activeKey);
    const activeIndex = Math.max(0, selectedIndex);
    const active = matches[activeIndex];
    // Keep a selected message visible when a newer matching message arrives.
    const visibleLimit = Math.max(limit, activeIndex + 1);
    const visibleMatches = matches.slice(0, visibleLimit);

    useEffect(() => {
        if (selectedIndex < 0) setActiveKey(matches[0]?.message.key ?? null);
    }, [matches, selectedIndex]);
    useEffect(() => {
        if (activeKey)
            resultsRef.current
                ?.querySelector<HTMLElement>('[aria-selected="true"]')
                ?.scrollIntoView?.({ block: 'nearest' });
    }, [activeKey, activeIndex]);

    const resetQuery = (value: string) => {
        setQuery(value);
        setActiveKey(null);
        setLimit(RESULTS_PER_PAGE);
    };
    const resultsStatus = term
        ? matches.length
            ? `${matches.length} 条结果${visibleMatches.length < matches.length ? ` · 显示最近 ${visibleMatches.length} 条` : ''}`
            : `没有找到匹配消息${onLoadEarlier && !history?.done ? '，可换个关键词或加载更早消息' : '，试试其他关键词'}`
        : `${matches.length} 条记录${visibleMatches.length < matches.length ? ` · 显示最近 ${visibleMatches.length} 条` : ''}`;
    const historyStatus = history?.loading
        ? history.loaded
            ? '正在加载更早消息…'
            : '正在读取消息…'
        : history?.done
          ? '已到最早消息'
          : !canLoadEarlier && onLoadEarlier
            ? '连接恢复后可加载更早消息'
            : '';

    return (
        <section
            className="native-chat-message-search"
            aria-label="搜索当前会话的消息"
            onKeyDown={(event) => {
                if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229)
                    return;
                if (event.key === 'Escape') {
                    event.preventDefault();
                    event.stopPropagation();
                    onClose();
                }
            }}
        >
            <button
                type="button"
                className="native-chat-icon native-chat-search-close"
                aria-label="关闭消息搜索"
                title="关闭搜索 · Esc"
                onClick={onClose}
            >
                <X size={16} />
            </button>
            <div className="relative z-[1] flex shrink-0 items-center gap-2">
                <div className="native-chat-search min-w-0 flex-1">
                    <Search size={14} aria-hidden />
                    <input
                        ref={field}
                        autoFocus
                        role="combobox"
                        aria-label="搜索已加载消息"
                        aria-autocomplete="list"
                        aria-expanded={matches.length > 0}
                        aria-controls={`${id}-results`}
                        aria-describedby={`${id}-status ${id}-scope`}
                        aria-activedescendant={active ? `${id}-result-${activeIndex}` : undefined}
                        placeholder="搜索当前会话"
                        value={query}
                        onChange={(event) => resetQuery(event.target.value)}
                        onCompositionStart={() => {
                            composing.current = true;
                        }}
                        onCompositionEnd={() => {
                            composing.current = false;
                        }}
                        onKeyDown={(event) => {
                            if (
                                event.nativeEvent.isComposing ||
                                composing.current ||
                                event.keyCode === 229
                            )
                                return;
                            if (
                                (event.key === 'ArrowDown' || event.key === 'ArrowUp') &&
                                matches.length
                            ) {
                                event.preventDefault();
                                const nextIndex = Math.max(
                                    0,
                                    Math.min(
                                        matches.length - 1,
                                        activeIndex + (event.key === 'ArrowDown' ? 1 : -1),
                                    ),
                                );
                                setLimit((current) => Math.max(current, nextIndex + 1));
                                setActiveKey(matches[nextIndex].message.key);
                            } else if (event.key === 'Enter' && active) {
                                event.preventDefault();
                                onReveal(active.message.key);
                            }
                        }}
                    />
                    {query && (
                        <button
                            type="button"
                            aria-label="清除消息搜索"
                            onClick={() => {
                                resetQuery('');
                                field.current?.focus();
                            }}
                        >
                            <X size={13} />
                        </button>
                    )}
                </div>
            </div>
            <div className="native-chat-search-toolbar">
                <div role="tablist" aria-label="消息类型" className="native-chat-search-categories">
                    {categories.map(([value, label], index) => (
                        <button
                            type="button"
                            key={value}
                            role="tab"
                            aria-selected={category === value}
                            tabIndex={category === value ? 0 : -1}
                            onClick={() => {
                                setCategory(value);
                                setActiveKey(null);
                                setLimit(RESULTS_PER_PAGE);
                            }}
                            onKeyDown={(event) => {
                                if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
                                event.preventDefault();
                                const next =
                                    (index +
                                        (event.key === 'ArrowRight' ? 1 : -1) +
                                        categories.length) %
                                    categories.length;
                                setCategory(categories[next][0]);
                                setActiveKey(null);
                                setLimit(RESULTS_PER_PAGE);
                                event.currentTarget.parentElement
                                    ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
                                    [next]?.focus();
                            }}
                        >
                            {label}
                        </button>
                    ))}
                </div>
                <Popover>
                    <PopoverTrigger asChild>
                        <button
                            type="button"
                            className="native-chat-search-filter-trigger"
                            data-active={!!sender || !!from || !!to}
                        >
                            <Filter size={14} />
                            筛选
                        </button>
                    </PopoverTrigger>
                    <PopoverContent align="end" className="native-chat-search-filters">
                        <label>
                            发送者
                            <select
                                aria-label="筛选发送者"
                                value={sender}
                                onChange={(event) => {
                                    setSender(event.target.value);
                                    setActiveKey(null);
                                    setLimit(RESULTS_PER_PAGE);
                                }}
                            >
                                <option value="">所有人</option>
                                {senders.map(([id, name]) => (
                                    <option key={id} value={id}>
                                        {name}
                                    </option>
                                ))}
                            </select>
                        </label>
                        <div className="native-chat-search-date-fields">
                            <label>
                                起始日期
                                <input
                                    type="date"
                                    aria-label="起始日期"
                                    value={from}
                                    onChange={(event) => {
                                        setFrom(event.target.value);
                                        setActiveKey(null);
                                        setLimit(RESULTS_PER_PAGE);
                                    }}
                                />
                            </label>
                            <label>
                                结束日期
                                <input
                                    type="date"
                                    aria-label="结束日期"
                                    value={to}
                                    onChange={(event) => {
                                        setTo(event.target.value);
                                        setActiveKey(null);
                                        setLimit(RESULTS_PER_PAGE);
                                    }}
                                />
                            </label>
                        </div>
                        <button
                            type="button"
                            className="native-chat-text-button"
                            onClick={() => {
                                setSender('');
                                setFrom('');
                                setTo('');
                            }}
                        >
                            清除筛选
                        </button>
                    </PopoverContent>
                </Popover>
            </div>
            <div className="native-chat-search-summary">
                <p id={`${id}-status`} role="status">
                    {resultsStatus}
                    {historyStatus && ` · ${historyStatus}`}
                </p>
                <p id={`${id}-scope`}>
                    仅搜索当前会话已加载的 {searchable.length} 条消息
                    <span className="native-chat-search-key-hint"> · ↑↓ 选择 · Enter 定位</span>
                </p>
                {history?.error && (
                    <p role="alert" className="text-danger">
                        读取历史失败：{history.error}
                    </p>
                )}
            </div>
            <div
                ref={resultsRef}
                id={`${id}-results`}
                role="listbox"
                aria-label="消息搜索结果"
                className="native-chat-search-results"
            >
                {visibleMatches.map(({ message, preview }, index) => (
                    <Fragment key={message.key}>
                        {(index === 0 ||
                            dateGroup(visibleMatches[index - 1].message.at) !==
                                dateGroup(message.at)) && (
                            <div role="presentation" className="native-chat-search-date">
                                {dateGroup(message.at)}
                            </div>
                        )}
                        <div
                            role="option"
                            className="native-chat-search-result"
                            id={`${id}-result-${index}`}
                            aria-selected={message.key === active?.message.key}
                            aria-label={`${message.mine ? '我' : message.senderName} ${dayLabel(message.at)} ${preview}`}
                            tabIndex={-1}
                            onMouseDown={(event) => event.preventDefault()}
                            onClick={() => onReveal(message.key)}
                        >
                            <ChatAvatar
                                contact={{
                                    type: 'private',
                                    id: message.senderId,
                                    name: message.senderName,
                                }}
                                small
                            />
                            <div className="native-chat-search-result-body">
                                <div className="native-chat-search-result-meta">
                                    <span>{message.mine ? '我' : message.senderName}</span>
                                    <time>
                                        {new Date(message.at).toLocaleTimeString('zh-CN', {
                                            hour: '2-digit',
                                            minute: '2-digit',
                                        })}
                                    </time>
                                </div>
                                <SearchPicture message={message} />
                                <span className="native-chat-search-result-text">
                                    {pattern ? highlightPreview(preview, pattern) : preview}
                                </span>
                            </div>
                            <button
                                type="button"
                                className="native-chat-search-locate"
                                aria-label="定位这条消息"
                                title="在会话中定位"
                                onClick={(event) => {
                                    event.stopPropagation();
                                    onReveal(message.key);
                                }}
                            >
                                <MapPin size={15} />
                            </button>
                        </div>
                    </Fragment>
                ))}
                {!visibleMatches.length && (
                    <div className="native-chat-search-empty">
                        <Search size={24} strokeWidth={1.3} />
                        <p>{term ? '换个关键词试试' : '暂无这类消息'}</p>
                    </div>
                )}
            </div>
            {(visibleMatches.length < matches.length || (onLoadEarlier && !history?.done)) && (
                <div className="native-chat-search-footer">
                    {visibleMatches.length < matches.length && (
                        <button
                            type="button"
                            className="native-chat-text-button"
                            onClick={() => setLimit(visibleLimit + RESULTS_PER_PAGE)}
                        >
                            显示更多结果（还有 {matches.length - visibleMatches.length} 条）
                        </button>
                    )}
                    {onLoadEarlier && !history?.done && (
                        <button
                            type="button"
                            className="native-chat-text-button"
                            disabled={history?.loading || !canLoadEarlier}
                            onClick={onLoadEarlier}
                        >
                            {history?.loading
                                ? '正在加载…'
                                : history?.error
                                  ? '重试加载更早消息'
                                  : '加载更早消息'}
                        </button>
                    )}
                </div>
            )}
        </section>
    );
}
