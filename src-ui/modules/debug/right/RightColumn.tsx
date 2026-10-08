// 右栏内容：聊天视图 / 事件列表、会话条、轻量输入框。
//
// 事件的接收由页面负责（选中且在运行的 Bot 自动开始收），这里只读 debugEventStore 画出来。
// 「Bot 没在运行」的提示条由外框画在这一栏顶上。高度由外框给定，时间线在里面自己滚。
//
// 筛选、暂停、选中的气泡、打开的详情都是界面状态，放在这里；换 Bot 时整块按 Bot 重新挂（key），各管各的。
// 筛选条件跨 Bot 保留（记在模块里），收起右栏再展开也还在。

import {
    memo,
    useCallback,
    useDeferredValue,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from 'react';
import { MessagesSquare, RotateCw } from 'lucide-react';
import { Button, Dialog, DialogContent, DialogTitle } from '../../../shared/ui';
import {
    debugEventStore,
    useDebugActiveSession,
    useDebugChat,
    useDebugReceiverState,
} from '../../../hooks/debug/debugEventStore';
import {
    debugWorkspaceStore,
    useActiveDebugTab,
    useDebugChannelChoice,
    useDebugLayout,
} from '../../../hooks/debug/debugWorkspaceStore';
import { useDebugActionSpec } from '../../../hooks/debug/useDebugCatalog';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import {
    MAX_CHAT_ITEMS,
    type ChatItem,
    type ChatState,
    type SessionKey,
} from '../../../core/domain/debug/chat';
import {
    DEFAULT_CHAT_FILTER,
    filterItems,
    type ChatFilter,
} from '../../../core/domain/debug/chatFilter';
import { messagePreview } from '../../../core/domain/debug/segments';
import { buildFormModel, type FormField } from '../../../core/domain/debug/schemaForm';
import { targetDisplayName } from '../../../core/domain/debug/targetGroups';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugChatView } from '../../../core/ipc/generated/debug/DebugChatView';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { ChatViewContext, type ChatViewApi } from '../../../shared/chat/chatContext';
import {
    idsOfItem,
    parseSessionKey,
    planFill,
    type FillPlan,
    type MessageItem,
} from '../../../core/domain/debug/chatFormat';
import { callProblem } from '../../../core/domain/debug/errorCopy';
import { handleRequestCall } from '../../../core/domain/debug/requestHandling';
import { useDebugCall } from '../../../hooks/debug/useDebugCall';
import { ChatTimeline } from './ChatTimeline';
import { ChatToolbar, type KindFilter } from './ChatToolbar';
import { Composer, type ComposerReply, type ComposerTarget } from './Composer';
import { EventDetailPopover, type DetailTarget } from './EventDetailPopover';
import { EventListView } from './EventListView';
import { PausedPill } from './NewMessagesPill';
import { SessionStrip } from './SessionStrip';
import { createChatMediaService } from '../../../hooks/chat/useChatMedia';

export interface RightColumnProps {
    /** 当前选中的 Bot；没在运行时输入框禁用 */
    target: DebugTarget | null;
    /** 顶栏为这个 Bot 选的调用通道，输入框发消息走它 */
    callChannel: DebugChannelId;
}

export const RightColumn = memo(function RightColumn({ target, callChannel }: RightColumnProps) {
    if (!target) {
        return (
            <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
                <span className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-inset text-text-tertiary">
                    <MessagesSquare size={17} strokeWidth={1.9} aria-hidden />
                </span>
                <p className="max-w-[18rem] text-2xs leading-relaxed text-text-tertiary">
                    选中的 Bot 收到的消息、通知和调用会像 QQ 一样按会话显示在这里。
                </p>
            </div>
        );
    }
    return <BotChat key={target.bot_id} target={target} callChannel={callChannel} />;
});

// ---------------------------------------------------------------------------
// 模块里记着的界面偏好 / 状态
// ---------------------------------------------------------------------------

let lastKinds: KindFilter = DEFAULT_CHAT_FILTER.kinds;
let lastShowHeartbeat = DEFAULT_CHAT_FILTER.showHeartbeat;
/** 每个 Bot 清屏时的 lastSeq：「更早的已丢弃」只算清屏之后被挤掉的 */
const clearedAt = new Map<string, number>();

const AUTO: DebugChannelId = { kind: 'auto' };
const NO_FIELDS: FormField[] = [];

interface Selection {
    key: string;
    session: SessionKey;
    messageId?: number;
    senderName: string;
    preview: string;
}

function selectionOf(item: MessageItem): Selection {
    return {
        key: item.key,
        session: item.session,
        ...(item.messageId !== undefined ? { messageId: item.messageId } : {}),
        senderName: item.senderName,
        preview: messagePreview(item.segments),
    };
}

/** 按条目数组的身份缓存索引：数组每帧都换，但只有真有人来查时才建一次 */
function lazyIndex<V>(build: (items: readonly ChatItem[]) => Map<number, V>) {
    const cache = new WeakMap<readonly ChatItem[], Map<number, V>>();
    return (items: readonly ChatItem[]): Map<number, V> => {
        let idx = cache.get(items);
        if (!idx) {
            idx = build(items);
            cache.set(items, idx);
        }
        return idx;
    };
}

const messageIndex = lazyIndex<MessageItem>((items) => {
    const map = new Map<number, MessageItem>();
    for (const it of items)
        if (it.kind === 'message' && it.messageId !== undefined) map.set(it.messageId, it);
    return map;
});

// ---------------------------------------------------------------------------
// 一个 Bot 的聊天
// ---------------------------------------------------------------------------

function BotChat({ target, callChannel }: { target: DebugTarget; callChannel: DebugChannelId }) {
    const botId = target.bot_id;
    const running = target.running;
    const openExternal = useOpenExternal();
    const view: DebugChatView = useDebugLayout().right_view;
    const chat = useDebugChat(botId);
    const activeSession = useDebugActiveSession(botId);
    const receiver = useDebugReceiverState(botId);
    const eventsChannel = useDebugChannelChoice(botId)?.events ?? AUTO;

    // ---- 筛选
    const [kinds, setKinds] = useState<KindFilter>(lastKinds);
    const [showHeartbeat, setShowHeartbeat] = useState(lastShowHeartbeat);
    const [search, setSearch] = useState('');
    const deferredSearch = useDeferredValue(search);
    const onKindsChange = useCallback((k: KindFilter, hb: boolean) => {
        lastKinds = k;
        lastShowHeartbeat = hb;
        setKinds(k);
        setShowHeartbeat(hb);
    }, []);
    const resetFilter = useCallback(
        () => onKindsChange(DEFAULT_CHAT_FILTER.kinds, DEFAULT_CHAT_FILTER.showHeartbeat),
        [onKindsChange],
    );
    const filterActive =
        showHeartbeat !== DEFAULT_CHAT_FILTER.showHeartbeat ||
        (Object.keys(kinds) as Array<keyof KindFilter>).some(
            (k) => kinds[k] !== DEFAULT_CHAT_FILTER.kinds[k],
        );

    // ---- 暂停：画面停在按下那一刻，store 照常收
    const [frozen, setFrozen] = useState<{ items: ChatItem[]; lastSeq: number } | null>(null);
    const [resumes, setResumes] = useState(0);
    const frozenRef = useRef(frozen);
    frozenRef.current = frozen;
    const togglePause = useCallback(() => {
        if (frozenRef.current) {
            setFrozen(null);
            setResumes((n) => n + 1);
            return;
        }
        const c = debugEventStore.getSnapshot().bots[botId]?.chat;
        setFrozen({ items: c?.items ?? [], lastSeq: c?.lastSeq ?? 0 });
    }, [botId]);

    // ---- 清屏
    const [clears, setClears] = useState(0);
    // 输入框的两样东西分开记：「发到哪个会话」（「全部」视图里点气泡定下的）和「回复哪一条」（选中的气泡）。
    // 取消回复、发出去之后只清回复，发往的会话还在，接着说话不用再点一次气泡
    const [sendSession, setSendSession] = useState<SessionKey | null>(null);
    const [replyTo, setReplyTo] = useState<Selection | null>(null);
    const clear = useCallback(() => {
        const c = debugEventStore.getSnapshot().bots[botId]?.chat;
        clearedAt.set(botId, c?.lastSeq ?? 0);
        debugEventStore.clearChat(botId);
        setFrozen(null);
        setSendSession(null);
        setReplyTo(null);
        setClears((n) => n + 1);
    }, [botId]);

    const filter = useMemo<ChatFilter>(
        () => ({ session: activeSession, kinds, showHeartbeat, text: deferredSearch }),
        [activeSession, kinds, showHeartbeat, deferredSearch],
    );
    const source = frozen?.items ?? chat.items;
    const items = useMemo(() => filterItems(source, filter), [source, filter]);

    // 没见到也再要不回来的：后端环形缓冲在我们离开期间挤掉的，和本地时间线超过上限后挤掉的，
    // 取大的；清屏之前的不算。不能拿接收器的 first_seq - 1 算：seq 跨接收器重启连续编号，
    // 重启后那些「更早的」其实还在时间线上，那样算会误报「已丢弃」
    const first = chat.items[0];
    const localDropped = chat.items.length >= MAX_CHAT_ITEMS && first ? first.seq - 1 : 0;
    const trimmed = Math.max(
        0,
        Math.max(receiver.unseenDropped, localDropped) - (clearedAt.get(botId) ?? 0),
    );

    // ---- 换到某个会话：「全部」里定下的发往会话作废；不属于新会话的回复也作废
    useEffect(() => {
        if (activeSession === 'all') return;
        setSendSession(null);
        setReplyTo((r) => (r && r.session !== activeSession ? null : r));
    }, [activeSession]);

    // ---- 给每一行用的查询与操作（引用不变，内部读 ref）
    const chatRef = useRef<ChatState>(chat);
    chatRef.current = chat;
    const selfName = targetDisplayName(target);
    const selfRef = useRef({ id: target.qq_id > 0 ? target.qq_id : undefined, name: selfName });
    selfRef.current = {
        id: chat.selfId ?? (target.qq_id > 0 ? target.qq_id : undefined),
        name: selfName,
    };
    // 请求卡片的「同意 / 拒绝」：发调用要看当下的 Bot 和通道，记 ref 不记在 memo 依赖里
    const botRef = useRef({ id: target.bot_id, name: selfName });
    botRef.current = { id: target.bot_id, name: selfName };
    const callChannelRef = useRef(callChannel);
    callChannelRef.current = callChannel;
    const { send: sendCall } = useDebugCall();

    const activeTab = useActiveDebugTab();
    const spec = useDebugActionSpec(target, activeTab?.action ? activeTab.action : null);
    const fields = useMemo(
        () => (spec.data ? buildFormModel(spec.data.params_schema).fields : NO_FIELDS),
        [spec.data],
    );
    const tabRef = useRef(activeTab);
    tabRef.current = activeTab;
    const fieldsRef = useRef(fields);
    fieldsRef.current = fields;

    const [detail, setDetail] = useState<DetailTarget | null>(null);
    const [lightbox, setLightbox] = useState<string | null>(null);
    // 看过一次大图才挂对话框：平时不必每个 Bot 的聊天都挂着一个
    const [lightboxUsed, setLightboxUsed] = useState(false);
    if (lightbox !== null && !lightboxUsed) setLightboxUsed(true);
    const revealRef = useRef<((messageId: number) => boolean) | null>(null);
    const onRevealReady = useCallback((fn: ((messageId: number) => boolean) | null) => {
        revealRef.current = fn;
    }, []);
    const jumpRef = useRef<(() => void) | null>(null);
    const onJumpReady = useCallback((fn: (() => void) | null) => {
        jumpRef.current = fn;
    }, []);
    const focusComposer = useRef<(() => void) | null>(null);
    const expanded = useRef(new Set<string>());
    const mediaScope = JSON.stringify(['debug', botId, target.qq_id, target.backend, callChannel]);

    const api = useMemo<ChatViewApi>(() => {
        const media = createChatMediaService((_botId, action, params) =>
            sendCall(null, {
                bot_id: botId,
                channel: callChannel,
                action,
                params,
                timeout_ms: 30_000,
                origin: 'picker',
            }),
        );
        const previewFill = (item: ChatItem): FillPlan => {
            const tab = tabRef.current;
            if (!tab) return { ok: false, reason: '中间还没有打开请求标签' };
            if (!tab.action) return { ok: false, reason: '当前标签还没选接口' };
            return planFill(tab.params_text, fieldsRef.current, idsOfItem(item));
        };
        return {
            findMessage: (id) => messageIndex(chatRef.current.items).get(id),
            // reduce 时已经按号记好了名字（清屏也不丢），直接查表
            nameOf: (id) => {
                if (id === selfRef.current.id) return selfRef.current.name;
                return chatRef.current.names.get(id);
            },
            sessionName: (key) => chatRef.current.sessions[key]?.name,
            selfId: () => selfRef.current.id,
            bot: () => botRef.current,
            toggleSelect: (item) => {
                // 再点一次只是不回复它了；发往的会话留着
                setReplyTo((r) => (r?.key === item.key ? null : selectionOf(item)));
                setSendSession(item.session);
            },
            reply: (item) => {
                setReplyTo(selectionOf(item));
                setSendSession(item.session);
                requestAnimationFrame(() => focusComposer.current?.());
            },
            openDetail: (item, anchor) => setDetail({ item, anchor }),
            previewFill,
            fill: (item) => {
                const plan = previewFill(item);
                const tab = tabRef.current;
                if (plan.ok && tab) debugWorkspaceStore.setParamsText(tab.id, plan.text);
                return plan;
            },
            handleRequest: async (item, approve) => {
                const call = handleRequestCall(item, approve);
                if (!call) return { ok: false, reason: '这条请求没有 flag，处理不了' };
                const res = await sendCall(null, {
                    bot_id: botRef.current.id,
                    channel: callChannelRef.current,
                    action: call.action,
                    params: call.params,
                    timeout_ms: null,
                    origin: 'editor',
                });
                const problem = callProblem(res);
                return problem ? { ok: false, reason: problem } : { ok: true };
            },
            openImage: (url) => setLightbox(url),
            mediaScope,
            imageReadsQueued: true,
            isImageSourceAlive: (data) => media.isImageSourceAlive(target, data),
            readImage: (data, refresh, options) => media.image(target, data, refresh, options),
            readForward: (data) => media.forward(target, data),
            readRecord: (data) => media.record(target, data),
            readVideo: (data, refresh) => media.video(target, data, refresh),
            readRecordText: (messageId) => media.transcript(target, messageId),
            openLink: (url) => openExternal(url),
            revealMessage: (id) => revealRef.current?.(id) ?? false,
            isExpanded: (key) => expanded.current.has(key),
            setExpanded: (key, v) => {
                if (v) expanded.current.add(key);
                else expanded.current.delete(key);
            },
        };
        // 旧排队请求仍使用原账号和通道，切换 scope 后不会把结果写进新账号缓存。
    }, [openExternal, sendCall, mediaScope]);

    // ---- 输入框发往哪、回复谁
    const composerSessionKey =
        activeSession !== 'all' ? activeSession : (sendSession ?? replyTo?.session ?? null);
    const parsedSession = composerSessionKey ? parseSessionKey(composerSessionKey) : null;
    const composerName = composerSessionKey
        ? (chat.sessions[composerSessionKey]?.name ??
          (parsedSession?.type === 'group'
              ? `群 ${parsedSession.id}`
              : String(parsedSession?.id ?? '')))
        : '';
    const composerTo = useMemo<ComposerTarget | null>(
        () =>
            composerSessionKey && parsedSession
                ? {
                      session: composerSessionKey,
                      type: parsedSession.type,
                      id: parsedSession.id,
                      name: composerName,
                  }
                : null,
        // parsedSession 由 composerSessionKey 决定
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [composerSessionKey, composerName],
    );
    const reply = useMemo<ComposerReply | null>(
        () =>
            replyTo && replyTo.messageId !== undefined && replyTo.session === composerSessionKey
                ? {
                      messageId: replyTo.messageId,
                      senderName: replyTo.senderName,
                      preview: replyTo.preview,
                  }
                : null,
        [replyTo, composerSessionKey],
    );
    const clearReply = useCallback(() => setReplyTo(null), []);
    const dismissTarget = useCallback(() => {
        setSendSession(null);
        setReplyTo(null);
    }, []);
    // 发出去了：不再回复那条，回到最新，看得见自己的气泡和调用结果
    const onSent = useCallback(() => {
        setReplyTo(null);
        jumpRef.current?.();
    }, []);

    // ---- 接收（「重新接收」是用户的明确意图，走能解除手动停止标记的那条路）
    const stopReceiving = useCallback(() => debugEventStore.stopReceiving(botId), [botId]);
    const restartReceiving = useCallback(
        () =>
            debugEventStore.restartReceiving(
                botId,
                eventsChannel,
                target.qq_id > 0 ? target.qq_id : undefined,
            ),
        [botId, eventsChannel, target.qq_id],
    );

    const setView = useCallback(
        (v: DebugChatView) => debugWorkspaceStore.setLayout({ right_view: v }),
        [],
    );
    const selectSession = useCallback(
        (key: SessionKey | 'all') => debugEventStore.setActiveSession(botId, key),
        [botId],
    );

    const resetToken = `${activeSession}|${resumes}|${clears}`;
    const filterToken = `${Object.values(kinds).join('')}|${showHeartbeat}|${deferredSearch}`;
    const stopped = !receiver.subscribed || receiver.state?.state === 'stopped';

    let empty: ReactNode = null;
    if (items.length === 0) {
        if (source.length > 0) {
            empty = (
                <EmptyNote title="没有符合条件的事件">
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                            setSearch('');
                            resetFilter();
                            if (activeSession !== 'all') selectSession('all');
                        }}
                    >
                        清除筛选，看全部
                    </Button>
                </EmptyNote>
            );
        } else if (!running) {
            empty = (
                <EmptyNote
                    title="Bot 没在运行"
                    hint="启动之后，收到的消息、通知和调用会出现在这里。"
                />
            );
        } else if (stopped && receiver.state?.state === 'stopped') {
            empty = (
                <EmptyNote title="接收已停止" hint={receiver.state.reason || undefined}>
                    <Button size="sm" variant="secondary" onClick={() => void restartReceiving()}>
                        <RotateCw size={12} aria-hidden />
                        重新接收
                    </Button>
                </EmptyNote>
            );
        } else {
            empty = (
                <EmptyNote
                    title="还没有事件"
                    hint="群里说句话，或者在中间发个请求试试；新事件会从下面冒出来。"
                />
            );
        }
    }

    const pausedBar = frozen ? (
        <PausedPill pending={Math.max(0, chat.lastSeq - frozen.lastSeq)} onResume={togglePause} />
    ) : null;

    return (
        <ChatViewContext.Provider value={api}>
            <div className="@container/right flex min-h-0 flex-1 flex-col">
                <ChatToolbar
                    view={view}
                    onViewChange={setView}
                    kinds={kinds}
                    showHeartbeat={showHeartbeat}
                    onKindsChange={onKindsChange}
                    filterActive={filterActive}
                    onResetFilter={resetFilter}
                    search={search}
                    onSearchChange={setSearch}
                    matchCount={items.length}
                    paused={!!frozen}
                    onTogglePause={togglePause}
                    canClear={chat.items.length > 0}
                    onClear={clear}
                    receiver={receiver}
                    running={running}
                    onStopReceiving={stopReceiving}
                    onRestartReceiving={restartReceiving}
                />
                <SessionStrip
                    sessions={chat.sessions}
                    active={activeSession}
                    onSelect={selectSession}
                />
                {view === 'chat' ? (
                    <ChatTimeline
                        botId={botId}
                        items={items}
                        selectedKey={replyTo?.key ?? null}
                        showSessionName={activeSession === 'all'}
                        trimmed={trimmed}
                        resetToken={resetToken}
                        filterToken={filterToken}
                        paused={!!frozen}
                        empty={empty}
                        pausedBar={pausedBar}
                        onRevealReady={onRevealReady}
                        onJumpReady={onJumpReady}
                    />
                ) : (
                    <EventListView
                        botId={botId}
                        items={items}
                        activeKey={detail?.item.key ?? null}
                        trimmed={trimmed}
                        resetToken={resetToken}
                        filterToken={filterToken}
                        paused={!!frozen}
                        empty={empty}
                        pausedBar={pausedBar}
                    />
                )}
                {view === 'chat' && (
                    <Composer
                        target={target}
                        callChannel={callChannel}
                        to={composerTo}
                        showTarget={activeSession === 'all'}
                        reply={reply}
                        onClearReply={clearReply}
                        onDismissTarget={dismissTarget}
                        onSent={onSent}
                        focusRef={focusComposer}
                    />
                )}
            </div>
            <EventDetailPopover target={detail} onClose={() => setDetail(null)} />
            {lightboxUsed && (
                <ImageLightbox
                    url={lightbox}
                    onClose={() => setLightbox(null)}
                    onOpenLink={openExternal}
                />
            )}
        </ChatViewContext.Provider>
    );
}

function EmptyNote({
    title,
    hint,
    children,
}: {
    title: string;
    hint?: string;
    children?: ReactNode;
}) {
    return (
        <div className="flex max-w-[18rem] flex-col items-center gap-2 text-center">
            <span className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-inset text-text-tertiary">
                <MessagesSquare size={17} strokeWidth={1.9} aria-hidden />
            </span>
            <p className="font-display text-[13px] font-semibold text-text-secondary">{title}</p>
            {hint && <p className="text-2xs leading-relaxed text-text-tertiary">{hint}</p>}
            {children}
        </div>
    );
}

function ImageLightbox({
    url,
    onClose,
    onOpenLink,
}: {
    url: string | null;
    onClose: () => void;
    onOpenLink: (url: string) => void;
}) {
    const web = !!url && /^https?:/i.test(url);
    return (
        <Dialog open={url !== null} onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="lg">
                <DialogTitle className="sr-only">看大图</DialogTitle>
                {url && (
                    <div className="flex flex-col items-center gap-3">
                        <img
                            src={url}
                            alt=""
                            referrerPolicy="no-referrer"
                            className="max-h-[70vh] max-w-full rounded-md object-contain"
                        />
                        {web && (
                            <div className="flex gap-2">
                                <Button
                                    size="sm"
                                    variant="secondary"
                                    onClick={() =>
                                        void navigator.clipboard?.writeText(url).catch(() => {})
                                    }
                                >
                                    复制图片地址
                                </Button>
                                <Button size="sm" variant="ghost" onClick={() => onOpenLink(url)}>
                                    在浏览器里打开
                                </Button>
                            </div>
                        )}
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
}
