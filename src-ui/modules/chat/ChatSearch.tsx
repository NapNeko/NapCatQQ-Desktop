// 搜索当前会话及账号档案，定位后回到时间线。
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
import { ChevronDown, History, Filter, MapPin, Search, X } from 'lucide-react';
import {
    accountKey,
    type Contact,
    type Message,
    type SessionKey,
} from '../../core/domain/chat/model';
import { messagePreview, segmentPreview, type Segment } from '../../core/domain/debug/segments';
import { projectMessageDisplay } from '../../core/domain/chat/messageDisplay';
import { markdownContent } from '../../core/domain/debug/markdown';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { useMotion } from '../../hooks/preferences/useMotion';
import { ChatAvatar } from '../../shared/chat/ChatAvatar';
import { QQFace } from '../../shared/chat/media/QQFace';
import { SegmentList } from '../../shared/chat/SegmentView';
import { ChatViewContext, useChatView } from '../../shared/chat/chatContext';
import { useChatMedia } from '../../hooks/chat/useChatMedia';
import { useQQFaceLookup } from '../../hooks/chat/useChatQqFaces';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ChatImageViewer } from '../../shared/chat/ChatImageViewer';
import { Button } from '../../shared/ui/Button';
import { SimpleMarkdown } from '../../shared/ui/SimpleMarkdown';
import { Popover, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import './chat-search.css';

interface ChatSearchProps {
    target?: DebugTarget;
    messages: Message[];
    archivedMessages?: readonly Message[];
    currentSession?: SessionKey | null;
    conversations?: Readonly<Record<string, Contact>>;
    initialScope?: 'conversation' | 'account';
    initialSearchExpanded?: boolean;
    onReveal: (key: string) => void;
    onRevealMessage?: (message: Message) => void;
    onClose: () => void;
    inputRef?: RefObject<HTMLInputElement>;
    history?: { loading: boolean; loaded: boolean; done: boolean; error: string };
    onLoadEarlier?: () => void;
    canLoadEarlier?: boolean;
    preventRecall?: boolean;
}

const RESULTS_PER_PAGE = 50;
const EMPTY_MESSAGES: readonly Message[] = [];
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
        if (segment.type === 'markdown') return /https?:\/\//i.test(markdownContent(segment.data));
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

function mergeSearchMessages(archived: readonly Message[], loaded: readonly Message[]) {
    const byKey = new Map<string, Message>();
    const byId = new Map<string, string>();
    for (const source of [archived, loaded]) {
        for (const message of source) {
            const key = `${message.session}/${message.key}`;
            const identity = message.id ? `${message.session}/${message.id}` : undefined;
            const previousKey = identity ? byId.get(identity) : undefined;
            const previous = byKey.get(previousKey ?? key);
            if (previousKey && previousKey !== key) byKey.delete(previousKey);
            // 已撤回记录不能被迟到的旧历史复活。
            byKey.set(
                key,
                previous?.recalled
                    ? {
                          ...message,
                          recalled: true,
                          segments: previous.segments.length ? previous.segments : message.segments,
                      }
                    : message,
            );
            if (identity) byId.set(identity, key);
        }
    }
    return [...byKey.values()].sort((a, b) => a.at - b.at);
}

function SearchPicture({
    picture,
    messageId,
    imageIndex,
}: {
    picture: Segment;
    messageId?: string;
    imageIndex: number;
}) {
    return (
        <span className="native-chat-search-picture">
            <SegmentList
                segments={[picture]}
                mine={false}
                messageId={messageId}
                imageIndexOffset={imageIndex}
            />
        </span>
    );
}

function SearchContent({ message, pattern }: { message: Message; pattern: RegExp | null }) {
    const peekFace = useQQFaceLookup();
    let imageIndex = 0;
    return (
        <div className="native-chat-search-result-content">
            {projectMessageDisplay(message.segments, peekFace).map((segment, index) => {
                if (segment.type === 'image' || segment.type === 'mface')
                    return (
                        <SearchPicture
                            key={index}
                            picture={segment}
                            messageId={message.id}
                            imageIndex={imageIndex++}
                        />
                    );
                if (segment.type === 'face')
                    return (
                        <QQFace
                            key={index}
                            id={String(segment.data.id ?? '')}
                            data={segment.data}
                            displayLarge={segment.displayLarge}
                            animated
                            name={
                                typeof segment.data.name === 'string'
                                    ? segment.data.name
                                    : undefined
                            }
                            url={
                                typeof segment.data.url === 'string' ? segment.data.url : undefined
                            }
                        />
                    );
                if (segment.type === 'markdown')
                    return (
                        <SimpleMarkdown
                            key={index}
                            text={markdownContent(segment.data)}
                            emptyFallback="（空的 Markdown）"
                            className="native-chat-search-markdown"
                        />
                    );
                const preview = segmentPreview(segment);
                return (
                    <span key={index} className="native-chat-search-result-text">
                        {pattern ? highlightPreview(preview, pattern) : preview}
                    </span>
                );
            })}
        </div>
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
    target,
    messages,
    archivedMessages,
    currentSession,
    conversations,
    initialScope = 'conversation',
    initialSearchExpanded = false,
    onReveal,
    onRevealMessage,
    onClose,
    inputRef,
    history,
    onLoadEarlier,
    canLoadEarlier = true,
    preventRecall = false,
}: ChatSearchProps) {
    const fallback = useChatView();
    // target 为空时不向子组件注入读取回调，hook 的 getter 只会在 target 存在时被调用。
    const media = useChatMedia(() => target as DebugTarget);
    const [image, setImage] = useState('');
    useEffect(() => setImage(''), [target?.bot_id, target?.qq_id]);
    const view = useMemo(
        () => ({
            ...fallback,
            openImage: setImage,
            ...(target
                ? {
                      mediaScope: accountKey(target.bot_id, String(target.qq_id)),
                      imageReadsQueued: true,
                      isImageSourceAlive: media.isImageSourceAlive,
                      readImage: media.image,
                  }
                : {}),
        }),
        [fallback, target, media],
    );
    const motion = useMotion();
    const [query, setQuery] = useState('');
    const [searchExpanded, setSearchExpanded] = useState(initialSearchExpanded);
    const [scope, setScope] = useState(initialScope);
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const [limit, setLimit] = useState(RESULTS_PER_PAGE);
    const [category, setCategory] = useState<Category>('all');
    const [sender, setSender] = useState('');
    const [from, setFrom] = useState('');
    const [to, setTo] = useState('');
    const localInput = useRef<HTMLInputElement>(null);
    const searchToggle = useRef<HTMLButtonElement>(null);
    const resultsRef = useRef<HTMLDivElement>(null);
    const composing = useRef(false);
    const field = inputRef ?? localInput;
    const id = useId();
    const term = query.trim();
    const session = currentSession === undefined ? messages[0]?.session : currentSession;
    const hasArchive = archivedMessages !== undefined;
    const accountScope = hasArchive && (scope === 'account' || !session);
    const archive = archivedMessages ?? EMPTY_MESSAGES;
    const scopedMessages = useMemo(
        () =>
            accountScope
                ? mergeSearchMessages(archive, EMPTY_MESSAGES)
                : mergeSearchMessages(
                      archive.filter((message) => message.session === session),
                      messages.filter((message) => !session || message.session === session),
                  ),
        [accountScope, archive, messages, session],
    );
    const pattern = useMemo(
        () => (term ? new RegExp(term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'iu') : null),
        [term],
    );
    const searchable = useMemo(
        () =>
            scopedMessages
                .filter((message) => preventRecall || !message.recalled)
                .map((message) => ({
                    message,
                    preview: messagePreview(message.segments),
                    conversation:
                        conversations?.[message.session]?.name ||
                        `${message.session.startsWith('group:') ? '群聊' : '私聊'} ${message.session.slice(message.session.indexOf(':') + 1)}`,
                })),
        [scopedMessages, conversations, preventRecall],
    );
    const conversationCount = useMemo(
        () => new Set(searchable.map(({ message }) => message.session)).size,
        [searchable],
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
    const matches = useMemo(() => {
        const startAt = from ? new Date(`${from}T00:00:00`).getTime() : -Infinity;
        const endAt = to ? new Date(`${to}T23:59:59.999`).getTime() + 1 : Infinity;
        return searchable
            .filter(
                ({ message, preview }) =>
                    (!pattern || pattern.test(preview)) &&
                    inCategory(message, category) &&
                    (!sender || message.senderId === sender) &&
                    message.at >= startAt &&
                    message.at < endAt,
            )
            .reverse();
    }, [searchable, pattern, category, sender, from, to]);
    const selectedIndex = matches.findIndex((item) => item.message.key === activeKey);
    const activeIndex = Math.max(0, selectedIndex);
    const active = matches[activeIndex];
    // Keep a selected message visible when a newer matching message arrives.
    const visibleLimit = Math.max(limit, activeIndex + 1);
    const visibleMatches = matches.slice(0, visibleLimit);

    useEffect(() => {
        if (searchExpanded) field.current?.focus();
    }, [searchExpanded, field]);
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
    const changeScope = (value: typeof scope) => {
        setScope(value);
        setSender('');
        setActiveKey(null);
        setLimit(RESULTS_PER_PAGE);
    };
    const revealMessage = (message: Message) => {
        if (onRevealMessage) onRevealMessage(message);
        else onReveal(message.key);
    };
    const canExpandHistory = !accountScope && !!onLoadEarlier && !history?.done;
    const resultsStatus = term
        ? matches.length
            ? `${matches.length} 条结果${visibleMatches.length < matches.length ? ` · 显示最近 ${visibleMatches.length} 条` : ''}`
            : `没有找到匹配消息${canExpandHistory ? '，可换个关键词或加载更早消息' : '，试试其他关键词'}`
        : `${matches.length} 条记录${visibleMatches.length < matches.length ? ` · 显示最近 ${visibleMatches.length} 条` : ''}`;
    const historyStatus = accountScope
        ? ''
        : history?.loading
          ? history.loaded
              ? '正在加载更早消息…'
              : '正在读取消息…'
          : history?.done
            ? '已到最早消息'
            : !canLoadEarlier && onLoadEarlier
              ? '连接恢复后可加载更早消息'
              : '';

    return (
        <ChatViewContext.Provider value={view}>
            <section
                className="native-chat-message-search"
                data-motion={motion.enabled ? 'on' : 'off'}
                aria-label={accountScope ? '搜索本账号已保存的消息' : '搜索当前会话的消息'}
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
                <div className="native-chat-search-toolbar" data-search-open={searchExpanded}>
                    <div
                        role="tablist"
                        aria-label="消息类型"
                        className="native-chat-search-categories"
                    >
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
                                        ?.[next]?.focus();
                                }}
                            >
                                {label}
                            </button>
                        ))}
                    </div>
                    <button
                        ref={searchToggle}
                        type="button"
                        autoFocus={!initialSearchExpanded}
                        className="native-chat-icon native-chat-search-toggle"
                        aria-label="搜索消息"
                        aria-expanded={searchExpanded}
                        aria-controls={`${id}-query`}
                        title={searchExpanded ? '收起搜索' : '搜索消息'}
                        onClick={() => {
                            if (searchExpanded) {
                                resetQuery('');
                                searchToggle.current?.focus();
                            }
                            setSearchExpanded(!searchExpanded);
                        }}
                    >
                        <Search size={15} aria-hidden />
                    </button>
                    {searchExpanded && (
                        <div
                            id={`${id}-query`}
                            className="native-chat-search native-chat-search-query"
                        >
                            <Search size={14} aria-hidden />
                            <input
                                ref={field}
                                role="combobox"
                                aria-label={
                                    hasArchive
                                        ? accountScope
                                            ? '搜索本账号已保存消息'
                                            : '搜索当前会话消息'
                                        : '搜索已加载消息'
                                }
                                aria-autocomplete="list"
                                aria-expanded={matches.length > 0}
                                aria-controls={`${id}-results`}
                                aria-describedby={`${id}-status ${id}-scope`}
                                aria-activedescendant={
                                    active ? `${id}-result-${activeIndex}` : undefined
                                }
                                placeholder={accountScope ? '搜索本账号已保存记录' : '搜索当前会话'}
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
                                        revealMessage(active.message);
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
                    )}
                    {hasArchive && (
                        <div
                            role="tablist"
                            aria-label="搜索范围"
                            className="native-chat-search-scope"
                        >
                            {(['conversation', 'account'] as const).map((value, index) => (
                                <button
                                    type="button"
                                    key={value}
                                    role="tab"
                                    aria-selected={accountScope === (value === 'account')}
                                    disabled={value === 'conversation' && !session}
                                    tabIndex={accountScope === (value === 'account') ? 0 : -1}
                                    onClick={() => changeScope(value)}
                                    onKeyDown={(event) => {
                                        if (
                                            !session ||
                                            !['ArrowLeft', 'ArrowRight'].includes(event.key)
                                        )
                                            return;
                                        event.preventDefault();
                                        const next = index === 0 ? 1 : 0;
                                        changeScope(next === 0 ? 'conversation' : 'account');
                                        event.currentTarget.parentElement
                                            ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
                                            ?.[next]?.focus();
                                    }}
                                >
                                    {value === 'conversation' ? '当前会话' : '本账号'}
                                </button>
                            ))}
                        </div>
                    )}
                    <Popover>
                        <PopoverTrigger asChild>
                            <button
                                type="button"
                                className="native-chat-search-filter-trigger"
                                aria-label="筛选"
                                data-active={!!sender || !!from || !!to}
                            >
                                <Filter size={14} />
                                <span>筛选</span>
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
                        {accountScope
                            ? `本账号已保存的 ${searchable.length} 条消息 · ${conversationCount} 个会话`
                            : hasArchive
                              ? `当前会话已加载及已保存的 ${searchable.length} 条消息`
                              : `仅搜索当前会话已加载的 ${searchable.length} 条消息`}
                        <span className="native-chat-search-key-hint"> · ↑↓ 选择 · Enter 定位</span>
                    </p>
                    {!accountScope && history?.error && (
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
                    {visibleMatches.map(({ message, preview, conversation }, index) => (
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
                                data-recalled={message.recalled || undefined}
                                id={`${id}-result-${index}`}
                                aria-selected={message.key === active?.message.key}
                                aria-label={`${accountScope ? `${conversation} ` : ''}${message.mine ? '我' : message.senderName} ${dayLabel(message.at)} ${message.recalled ? '已撤回 ' : ''}${preview}`}
                                tabIndex={-1}
                                onMouseDown={(event) => event.preventDefault()}
                                onClick={() => revealMessage(message)}
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
                                        <span className="native-chat-search-result-origin">
                                            <span>{message.mine ? '我' : message.senderName}</span>
                                            {message.recalled && (
                                                <span className="native-chat-search-recalled">
                                                    已撤回
                                                </span>
                                            )}
                                            {accountScope && (
                                                <span
                                                    className="native-chat-search-result-session"
                                                    title={conversation}
                                                >
                                                    {conversation}
                                                </span>
                                            )}
                                        </span>
                                        <time>
                                            {new Date(message.at).toLocaleTimeString('zh-CN', {
                                                hour: '2-digit',
                                                minute: '2-digit',
                                            })}
                                        </time>
                                    </div>
                                    <SearchContent message={message} pattern={pattern} />
                                </div>
                                <button
                                    type="button"
                                    className="native-chat-search-locate"
                                    aria-label="定位这条消息"
                                    title="在会话中定位"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        revealMessage(message);
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
                {(visibleMatches.length < matches.length || canExpandHistory) && (
                    <div className="native-chat-search-footer">
                        {visibleMatches.length < matches.length && (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                className="native-chat-search-more"
                                aria-label={`显示更多结果（还有 ${matches.length - visibleMatches.length} 条）`}
                                onClick={() => setLimit(visibleLimit + RESULTS_PER_PAGE)}
                            >
                                <ChevronDown size={14} aria-hidden />
                                显示更多结果
                                <span className="native-chat-search-remaining">
                                    {matches.length - visibleMatches.length}
                                </span>
                            </Button>
                        )}
                        {canExpandHistory && (
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                className="native-chat-search-earlier"
                                disabled={history?.loading || !canLoadEarlier}
                                onClick={onLoadEarlier}
                            >
                                <History size={14} aria-hidden />
                                {history?.loading
                                    ? '正在加载…'
                                    : history?.error
                                      ? '重试加载更早消息'
                                      : '加载更早消息'}
                            </Button>
                        )}
                    </div>
                )}
            </section>
            <ChatImageViewer src={image} onClose={() => setImage('')} />
        </ChatViewContext.Provider>
    );
}
