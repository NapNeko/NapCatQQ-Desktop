// 搜索当前会话及账号档案，定位后回到时间线。
import { useEffect, useId, useMemo, useRef, useState, type RefObject } from 'react';
import { X } from 'lucide-react';
import {
    accountKey,
    type Contact,
    type Message,
    type SessionKey,
} from '../../core/domain/chat/model';
import {
    RESULTS_PER_PAGE,
    buildSearchPattern,
    buildSearchableMessages,
    collectSenders,
    countConversations,
    filterMatches,
    mergeSearchMessages,
    type Category,
} from '../../core/domain/chat/chatSearchModel';
import { useMotion } from '../../hooks/preferences/useMotion';
import { ChatViewContext, useChatView } from '../../shared/chat/chatContext';
import { useChatMedia } from '../../hooks/chat/useChatMedia';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ChatImageViewer } from '../../shared/chat/ChatImageViewer';
import { ChatSearchToolbar } from './ChatSearchToolbar';
import { ChatSearchFooter, ChatSearchResults, ChatSearchSummary } from './ChatSearchResults';
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

const EMPTY_MESSAGES: readonly Message[] = [];

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
    const pattern = useMemo(() => buildSearchPattern(term), [term]);
    const searchable = useMemo(
        () =>
            buildSearchableMessages(
                scopedMessages.filter((message) => preventRecall || !message.recalled),
                conversations,
            ),
        [scopedMessages, conversations, preventRecall],
    );
    const conversationCount = useMemo(() => countConversations(searchable), [searchable]);
    const senders = useMemo(() => collectSenders(searchable), [searchable]);
    const matches = useMemo(
        () => filterMatches(searchable, { pattern, category, sender, from, to }),
        [searchable, pattern, category, sender, from, to],
    );
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
    const changeCategory = (value: Category) => {
        setCategory(value);
        setActiveKey(null);
        setLimit(RESULTS_PER_PAGE);
    };
    const selectIndex = (nextIndex: number) => {
        setLimit((current) => Math.max(current, nextIndex + 1));
        setActiveKey(matches[nextIndex].message.key);
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
                <ChatSearchToolbar
                    id={id}
                    field={field}
                    composing={composing}
                    query={query}
                    searchExpanded={searchExpanded}
                    initialSearchExpanded={initialSearchExpanded}
                    category={category}
                    hasArchive={hasArchive}
                    accountScope={accountScope}
                    session={session}
                    matchesLength={matches.length}
                    activeIndex={activeIndex}
                    hasActive={!!active}
                    senders={senders}
                    sender={sender}
                    from={from}
                    to={to}
                    onExpandedChange={setSearchExpanded}
                    onCategoryChange={changeCategory}
                    onScopeChange={changeScope}
                    resetQuery={resetQuery}
                    onSelectIndex={selectIndex}
                    onSubmitActive={() => active && revealMessage(active.message)}
                    onSenderChange={(value) => {
                        setSender(value);
                        setActiveKey(null);
                        setLimit(RESULTS_PER_PAGE);
                    }}
                    onFromChange={(value) => {
                        setFrom(value);
                        setActiveKey(null);
                        setLimit(RESULTS_PER_PAGE);
                    }}
                    onToChange={(value) => {
                        setTo(value);
                        setActiveKey(null);
                        setLimit(RESULTS_PER_PAGE);
                    }}
                    onClearFilters={() => {
                        setSender('');
                        setFrom('');
                        setTo('');
                    }}
                />
                <ChatSearchSummary
                    id={id}
                    resultsStatus={resultsStatus}
                    historyStatus={historyStatus}
                    accountScope={accountScope}
                    historyError={history?.error ?? ''}
                    searchableCount={searchable.length}
                    conversationCount={conversationCount}
                    hasArchive={hasArchive}
                />
                <ChatSearchResults
                    id={id}
                    resultsRef={resultsRef}
                    visibleMatches={visibleMatches}
                    activeMessageKey={active?.message.key}
                    accountScope={accountScope}
                    pattern={pattern}
                    term={term}
                    onReveal={revealMessage}
                />
                <ChatSearchFooter
                    remaining={matches.length - visibleMatches.length}
                    canExpandHistory={canExpandHistory}
                    historyLoading={!!history?.loading}
                    historyError={history?.error ?? ''}
                    canLoadEarlier={canLoadEarlier}
                    onLoadEarlier={onLoadEarlier}
                    onShowMore={() => setLimit(visibleLimit + RESULTS_PER_PAGE)}
                />
            </section>
            <ChatImageViewer src={image} onClose={() => setImage('')} />
        </ChatViewContext.Provider>
    );
}
