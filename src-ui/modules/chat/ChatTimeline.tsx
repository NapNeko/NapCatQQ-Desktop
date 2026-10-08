// 虚拟时间线复用消息段渲染和贴底逻辑，样式与阅读状态独立。
import {
    memo,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    type MutableRefObject,
    type ReactNode,
} from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown } from 'lucide-react';
import { ChatViewContext, useChatView } from '../../shared/chat/chatContext';
import { useStickToBottom } from '../../hooks/useStickToBottom';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { accountKey, type Contact, type Message } from '../../core/domain/chat/model';
import { useChatMedia } from '../../hooks/chat/useChatMedia';
import { useChatSend } from '../../hooks/chat/useChatSend';
import { ChatImageViewer } from '../../shared/chat/ChatImageViewer';
import { ChatAvatar as Avatar } from '../../shared/chat/ChatAvatar';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { cn } from '../../shared/utils/cn';
import { useHistoryPaging } from './useHistoryPaging';
import { ChatMessageActions } from './ChatMessageActions';
import { ChatAvatarMenu } from './ChatAvatarMenu';
import { MessageSkeleton } from './ChatSkeleton';
import { preserveTimelineReading } from '../../shared/chat/timelineReadingAnchor';
import { ChatMessageEntrance, ChatPresence } from './chatMotion';
import { ChatSendStatus } from './ChatSendStatus';
import { useTimelinePosition } from './useTimelinePosition';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { errorText } from '../../core/domain/errors';
import { useSmoothWheel } from './useSmoothWheel';
import { useLatestScroll } from './useLatestScroll';
import { ChatFileAction } from './files/ChatFileAction';
import { FORWARD_MESSAGE_LIMIT } from '../../core/domain/chat/messageTransfer';
import { ChatForwardDialog } from './ChatForwardDialog';
import { ChatMessageContent } from './ChatMessageContent';
import { ChatSelectionBar } from './ChatSelectionBar';

export function NativeTimeline({
    store,
    contact,
    messages,
    revealRef,
    visible,
    onFocusComposer,
    preventRecall = false,
    renderAvatar,
    onSelectionChange,
}: {
    store: ChatAccountStore;
    contact: Contact;
    messages: Message[];
    revealRef: MutableRefObject<(key: string) => void>;
    visible: boolean;
    onFocusComposer?: () => void;
    preventRecall?: boolean;
    renderAvatar?: (message: Message, avatar: ReactNode) => ReactNode;
    onSelectionChange?: (selecting: boolean) => void;
}) {
    const scroll = useRef<HTMLDivElement>(null);
    const latest = useRef(messages);
    latest.current = messages;
    const canPinToEnd = useRef<() => boolean>(() => true);
    const [image, showImage] = useState('');
    const [error, setError] = useState('');
    const [highlight, setHighlight] = useState('');
    const [selecting, setSelecting] = useState(false);
    const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
    const [forwardKeys, setForwardKeys] = useState<string[]>([]);
    const snapshot = useChatSnapshot(store);
    // target 经 hook 内部 ref 每次调用时求值，与原闭包直读 store.target 的时机一致。
    const media = useChatMedia(() => store.target);
    const chatSend = useChatSend(store);
    const history = snapshot.history[contact.key];
    useEffect(() => {
        void store.ensureHistory(contact.key);
    }, [store, contact.key, snapshot.connection.state, history]);
    const motion = useMotion();
    const sending = useMemo(
        () => messages.some((message) => message.status === 'sending'),
        [messages],
    );
    const retryDisabled =
        !store.target.running ||
        store.target.online === false ||
        snapshot.connection.state !== 'connected' ||
        sending;
    const getMessageKey = useCallback((index: number) => messages[index]?.key ?? index, [messages]);
    // 按消息 key 锚定 prepend 和异步行高变化，避免额外 scrollHeight 补偿重复移动视口。
    const pinLatest = useCallback(() => {
        const element = scroll.current;
        if (element && canPinToEnd.current())
            element.scrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
    }, []);
    const virtual = useVirtualizer({
        count: messages.length,
        getScrollElement: () => scroll.current,
        estimateSize: () => 80,
        getItemKey: getMessageKey,
        overscan: 8,
        paddingStart: 12,
        paddingEnd: selecting ? 96 : 24,
        anchorTo: 'end',
        directDomUpdates: true,
        onChange: pinLatest,
    });
    // 消息插入时先提交新容器高度，防止旧页锚点被旧 scrollHeight 截断。
    const containerRef = useCallback(
        (node: HTMLDivElement | null) => virtual.containerRef(node),
        [virtual, messages],
    );
    preserveTimelineReading(virtual, scroll);
    const key = `${accountKey(store.target.bot_id, String(store.target.qq_id))}/${contact.key}`;
    useEffect(() => {
        setSelecting(false);
        setSelectedKeys([]);
        setForwardKeys([]);
    }, [key]);
    useEffect(() => {
        onSelectionChange?.(selecting);
    }, [selecting, onSelectionChange]);
    useEffect(() => () => onSelectionChange?.(false), [onSelectionChange]);
    const beginSelection = useCallback((messageKey: string) => {
        setSelecting(true);
        setSelectedKeys([messageKey]);
    }, []);
    const toggleSelection = useCallback((messageKey: string) => {
        setSelectedKeys((selected) =>
            selected.includes(messageKey)
                ? selected.filter((value) => value !== messageKey)
                : selected.length < FORWARD_MESSAGE_LIMIT
                  ? [...selected, messageKey]
                  : selected,
        );
    }, []);
    const forwardOne = useCallback((messageKey: string) => setForwardKeys([messageKey]), []);
    useChatNotice(`${key}:action`, `${contact.name} · 消息操作失败`, error);
    const savedPosition = useRef(store.initialReadingPosition(contact.key));
    const stick = useStickToBottom({
        scrollRef: scroll,
        virtualizer: virtual,
        items: messages,
        memoryKey: null,
        initialDetached: !!savedPosition.current && !savedPosition.current.atBottom,
        reattachOnIntent: true,
        followUntilUserScroll: true,
        resetToken: key,
        filterToken: '',
        animate:
            motion.enabled &&
            !!history?.loaded &&
            !history.loading &&
            (scroll.current?.clientWidth ?? 0) > 0,
    });
    canPinToEnd.current = stick.canPinToEnd;
    const cancelWheel = useSmoothWheel(scroll, motion.enabled && visible);
    const latestScroll = useLatestScroll(scroll, {
        enabled: motion.enabled && visible,
        duration: motion.duration('slow') * 1000,
        detach: stick.detach,
        finish: () => stick.jumpToLatest(false),
    });
    const position = useTimelinePosition(
        store,
        contact.key,
        scroll,
        virtual,
        messages,
        stick.isFollowing,
        latestScroll.isRunning,
    );
    const loadEarlier = () => {
        position.capture();
        position.cancelRestore();
        stick.detach();
        return store.history(contact.key);
    };
    useChatNotice(
        `${key}:history`,
        `${contact.name} · 历史消息读取失败`,
        history?.error,
        () => void loadEarlier(),
    );
    const paging = useHistoryPaging({
        scroll,
        enabled:
            snapshot.connection.state === 'connected' &&
            !!history?.loaded &&
            !history.loading &&
            !history.done &&
            !history.error,
        load: loadEarlier,
        detach: stick.detach,
    });
    useEffect(() => {
        const update = () =>
            store.setReading(
                !stick.away &&
                    document.hasFocus() &&
                    document.visibilityState === 'visible' &&
                    (scroll.current?.clientWidth ?? 0) > 0
                    ? contact.key
                    : null,
            );
        update();
        document.addEventListener('visibilitychange', update);
        window.addEventListener('focus', update);
        window.addEventListener('blur', update);
        const observer = new ResizeObserver(update);
        if (scroll.current) observer.observe(scroll.current);
        return () => {
            document.removeEventListener('visibilitychange', update);
            window.removeEventListener('focus', update);
            window.removeEventListener('blur', update);
            observer.disconnect();
            store.setReading(null);
        };
    }, [store, contact.key, stick.away, visible]);
    useEffect(() => {
        revealRef.current = (messageKey) => {
            const index = latest.current.findIndex((m) => m.key === messageKey);
            if (index < 0) return;
            cancelWheel();
            latestScroll.cancel();
            position.cancelRestore();
            stick.detach();
            virtual.scrollToIndex(index, { align: 'center' });
            setHighlight(messageKey);
        };
        return () => {
            revealRef.current = () => {};
        };
    }, [
        revealRef,
        virtual,
        stick.detach,
        position.cancelRestore,
        cancelWheel,
        latestScroll.cancel,
    ]);
    useEffect(() => {
        if (!highlight) return;
        const timer = setTimeout(() => setHighlight(''), 1600);
        return () => clearTimeout(timer);
    }, [highlight]);
    const fallback = useChatView();
    const senderNames = useMemo(() => {
        const names = new Map<string, string>();
        for (const message of snapshot.account.messages)
            names.set(message.senderId, message.senderName);
        return names;
    }, [snapshot.account.messages]);
    const messageById = useMemo(
        () =>
            new Map(
                messages.filter((message) => message.id).map((message) => [message.id!, message]),
            ),
        [messages],
    );
    const view = useMemo(
        () => ({
            ...fallback,
            mediaScope: accountKey(store.target.bot_id, String(store.target.qq_id)),
            openImage: showImage,
            imageReadsQueued: true,
            isImageSourceAlive: media.isImageSourceAlive,
            readImage: media.image,
            readForward: media.forward,
            readRecord: media.record,
            readVideo: media.video,
            readRecordText: media.transcript,
            fileAction: (data: Record<string, unknown>) => (
                <ChatFileAction data={data} target={store.target} contact={contact} />
            ),
            openLink: (url: string) => {
                void chatSend.openLink(url).catch((e) => setError(String(e)));
            },
            nameOf: (userId: number) => senderNames.get(String(userId)),
            findMessage: (messageId: number) => {
                const m = messageById.get(String(messageId));
                return m
                    ? {
                          kind: 'message' as const,
                          key: m.key,
                          seq: 0,
                          at: m.at,
                          session: m.session,
                          direction: m.mine ? ('out' as const) : ('in' as const),
                          senderId: Number(m.senderId),
                          senderName: m.senderName,
                          messageId,
                          segments: m.segments,
                          raw: {},
                      }
                    : undefined;
            },
            revealMessage: (messageId: number) => {
                const m = messageById.get(String(messageId));
                if (!m) return false;
                revealRef.current(m.key);
                return true;
            },
        }),
        [fallback, senderNames, messageById, revealRef, store, contact, media, chatSend],
    );
    const latestLabel = stick.unseen > 0 ? `回到最新，${stick.unseen} 条新消息` : '回到最新';
    return (
        <ChatViewContext.Provider value={view}>
            <div
                className="native-chat-timeline-wrap"
                data-selecting={selecting}
                onKeyDown={(event) => {
                    if (
                        selecting &&
                        event.key === 'Escape' &&
                        !event.defaultPrevented &&
                        !event.nativeEvent.isComposing
                    ) {
                        event.preventDefault();
                        setSelecting(false);
                        setSelectedKeys([]);
                    }
                }}
            >
                <div
                    ref={scroll}
                    className="native-chat-timeline"
                    tabIndex={0}
                    aria-label="消息记录"
                    aria-busy={!!history?.loading}
                    onScroll={() => {
                        if (!position.restoring() && !latestScroll.isRunning())
                            stick.handlers.onScroll();
                        if (!latestScroll.isRunning()) {
                            paging.onScroll();
                            position.capture();
                        }
                    }}
                    onWheel={(e) => {
                        latestScroll.cancel();
                        position.cancelRestore();
                        stick.handlers.onWheel(e);
                        paging.onWheel(e);
                    }}
                    onKeyDown={(e) => {
                        if (
                            [
                                'ArrowUp',
                                'ArrowDown',
                                'PageUp',
                                'PageDown',
                                'Home',
                                'End',
                                ' ',
                            ].includes(e.key)
                        ) {
                            latestScroll.cancel();
                            position.cancelRestore();
                        }
                        stick.handlers.onKeyDown(e);
                        paging.onKeyDown(e);
                    }}
                    onPointerDown={(e) => {
                        latestScroll.cancel();
                        if (e.target === e.currentTarget) position.cancelRestore();
                        stick.handlers.onPointerDown(e);
                        if (e.target === e.currentTarget) paging.onPointerDown();
                    }}
                    onTouchStart={(e) => {
                        latestScroll.cancel();
                        position.cancelRestore();
                        stick.handlers.onTouchStart(e);
                        paging.onTouchStart(e);
                    }}
                    onTouchMove={(e) => {
                        stick.handlers.onTouchMove(e);
                        paging.onTouchMove(e);
                    }}
                >
                    {!messages.length &&
                        (history?.loading ? (
                            <MessageSkeleton />
                        ) : (
                            <div className="native-chat-message-empty">
                                还没有消息，从一句问候开始
                            </div>
                        ))}
                    <div ref={containerRef} style={{ position: 'relative' }}>
                        {virtual.getVirtualItems().map((row) => {
                            const message = messages[row.index];
                            return (
                                <div
                                    key={row.key}
                                    data-index={row.index}
                                    data-message-key={message.key}
                                    ref={virtual.measureElement}
                                    style={{ position: 'absolute', top: 0, left: 0, width: '100%' }}
                                >
                                    <TimelineMessage
                                        store={store}
                                        contact={contact}
                                        message={message}
                                        previous={messages[row.index - 1]}
                                        highlighted={highlight === message.key}
                                        retryDisabled={retryDisabled}
                                        preventRecall={preventRecall}
                                        renderAvatar={renderAvatar}
                                        onFocusComposer={onFocusComposer}
                                        onError={setError}
                                        selecting={selecting}
                                        selected={selectedKeys.includes(message.key)}
                                        selectionFull={selectedKeys.length >= FORWARD_MESSAGE_LIMIT}
                                        onSelect={beginSelection}
                                        onToggleSelection={toggleSelection}
                                        onForward={forwardOne}
                                        onLoadGap={
                                            message.id
                                                ? () => {
                                                      cancelWheel();
                                                      latestScroll.cancel();
                                                      position.cancelRestore();
                                                      stick.detach();
                                                      virtual.scrollToIndex(row.index, {
                                                          align: 'start',
                                                      });
                                                      void store.history(contact.key, message.id);
                                                  }
                                                : undefined
                                        }
                                        historyLoading={!!history?.loading}
                                        enter={row.index >= stick.enterFrom}
                                        takeEnter={stick.takeEnter}
                                        order={
                                            Number.isFinite(stick.enterFrom)
                                                ? row.index - stick.enterFrom
                                                : 0
                                        }
                                    />
                                </div>
                            );
                        })}
                    </div>
                </div>
                <ChatPresence visible={stick.away || stick.unseen > 0 || latestScroll.running}>
                    <button
                        className="native-chat-latest"
                        data-scrolling={latestScroll.running}
                        aria-label={latestLabel}
                        title={latestLabel}
                        onClick={() => {
                            position.cancelRestore();
                            paging.reset();
                            cancelWheel();
                            latestScroll.start((signal) => store.latest(contact.key, signal));
                        }}
                    >
                        <ArrowDown size={18} aria-hidden />
                        {stick.unseen > 0 && (
                            <span className="native-chat-latest-dot" aria-hidden />
                        )}
                    </button>
                </ChatPresence>
                <ChatSelectionBar
                    visible={selecting}
                    count={selectedKeys.length}
                    disabled={retryDisabled}
                    onCancel={() => {
                        setSelecting(false);
                        setSelectedKeys([]);
                    }}
                    onForward={() => setForwardKeys(selectedKeys)}
                />
            </div>
            <ChatImageViewer src={image} onClose={() => showImage('')} />
            {forwardKeys.length > 0 && (
                <ChatForwardDialog
                    store={store}
                    messageKeys={forwardKeys}
                    onClose={() => setForwardKeys([])}
                    onSent={() => {
                        setForwardKeys([]);
                        setSelecting(false);
                        setSelectedKeys([]);
                    }}
                />
            )}
        </ChatViewContext.Provider>
    );
}

const TimelineMessage = memo(function TimelineMessage({
    store,
    contact,
    message,
    previous,
    highlighted,
    retryDisabled,
    preventRecall,
    renderAvatar,
    onFocusComposer,
    onError,
    selecting,
    selected,
    selectionFull,
    onSelect,
    onToggleSelection,
    onForward,
    onLoadGap,
    historyLoading,
    enter,
    takeEnter,
    order,
}: {
    store: ChatAccountStore;
    contact: Contact;
    message: Message;
    previous?: Message;
    highlighted: boolean;
    retryDisabled: boolean;
    preventRecall: boolean;
    renderAvatar?: (message: Message, avatar: ReactNode) => ReactNode;
    onFocusComposer?: () => void;
    onError: (error: string) => void;
    selecting: boolean;
    selected: boolean;
    selectionFull: boolean;
    onSelect: (messageKey: string) => void;
    onToggleSelection: (messageKey: string) => void;
    onForward: (messageKey: string) => void;
    onLoadGap?: () => void;
    historyLoading: boolean;
    enter: boolean;
    takeEnter: (key: string) => boolean;
    order: number;
}) {
    const showTime =
        !previous ||
        message.at - previous.at > 5 * 60_000 ||
        new Date(message.at).toDateString() !== new Date(previous.at).toDateString();
    const continuation =
        !showTime &&
        previous?.senderId === message.senderId &&
        previous?.mine === message.mine &&
        message.at - previous.at < 3 * 60_000;
    const gap = message.gapBefore && (
        <div className="native-chat-notice">
            <button
                type="button"
                className="native-chat-text-button"
                disabled={historyLoading || !onLoadGap}
                onClick={onLoadGap}
            >
                {historyLoading ? '正在加载…' : '加载这段消息'}
            </button>
        </div>
    );
    if (message.notice) {
        return (
            <>
                {gap}
                {showTime && <div className="native-chat-time">{dayLabel(message.at)}</div>}
                <div className="native-chat-notice">{message.notice}</div>
            </>
        );
    }
    const avatar = (
        <Avatar
            contact={{
                type: 'private',
                id: message.senderId,
                name: message.mine ? store.target.name : message.senderName,
            }}
            small
        />
    );
    return (
        <>
            {gap}
            {showTime && <div className="native-chat-time">{dayLabel(message.at)}</div>}
            <ChatMessageEntrance
                messageKey={message.key}
                mine={message.mine}
                enter={enter}
                takeEnter={takeEnter}
                order={order}
            >
                <ChatMessageActions
                    store={store}
                    contact={contact}
                    message={message}
                    onFocusComposer={onFocusComposer}
                    onError={onError}
                    onForward={onForward}
                    onSelect={onSelect}
                    selecting={selecting}
                    selected={selected}
                    selectionDisabled={!selected && selectionFull}
                    onToggleSelection={onToggleSelection}
                >
                    {(controls, wrapContent) => (
                        <article
                            className={cn(
                                'native-chat-message',
                                message.mine && 'is-mine',
                                continuation && 'is-continuation',
                                showTime && 'is-after-time',
                            )}
                            data-highlight={highlighted}
                            data-selected={selected}
                            data-selecting={selecting}
                            data-local-send={!!message.requestId}
                        >
                            {continuation ? (
                                <span className="native-chat-avatar-space" aria-hidden />
                            ) : renderAvatar ? (
                                renderAvatar(message, avatar)
                            ) : (
                                <ChatAvatarMenu
                                    store={store}
                                    contact={contact}
                                    message={message}
                                    onFocusComposer={onFocusComposer}
                                    onError={onError}
                                >
                                    {avatar}
                                </ChatAvatarMenu>
                            )}
                            <div className="native-chat-message-body">
                                {contact.type === 'group' && !message.mine && !continuation && (
                                    <div className="native-chat-sender" title={message.senderName}>
                                        {message.senderName}
                                    </div>
                                )}
                                <div className="native-chat-message-content-line">
                                    {wrapContent(
                                        <ChatMessageContent
                                            message={message}
                                            preventRecall={preventRecall}
                                        />,
                                    )}
                                    {controls}
                                </div>
                                {message.mine && (
                                    <ChatSendStatus
                                        status={message.status}
                                        error={message.error}
                                        retryDisabled={retryDisabled || selecting}
                                        onRetry={
                                            message.recalled ||
                                            message.segments.some(
                                                (segment) => segment.type === 'forward',
                                            )
                                                ? undefined
                                                : () => {
                                                      onError('');
                                                      void store
                                                          .retry(message.key)
                                                          .catch((error) =>
                                                              onError(errorText(error)),
                                                          );
                                                  }
                                        }
                                    />
                                )}
                            </div>
                        </article>
                    )}
                </ChatMessageActions>
            </ChatMessageEntrance>
        </>
    );
});
