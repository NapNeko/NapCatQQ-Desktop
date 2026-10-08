// 调试台的事件流：每个 Bot 一份聊天时间线 + 接收器状态，模块级 store。
//
// 为什么在模块里而不是页面里：离开调试台再回来，聊天记录、未读数、接收状态都要在（frontend.md 坑 6）；
// 后端也是离开页面后继续收，回来时 subscribe 先补缓冲里已有的，reduceEvents 按 seq 去重，重复补不会出重复气泡。
//
// 后端约每 50ms 推一批，这里先攒进队列，一帧最多 reduce + setState 一次。
// 后端的空闲清扫会给每个订阅者发空批次探活，空批次什么都不做：不排帧、不 reduce、不通知。
//
// 事件里的调用记录不带 OB11 的 wording。本应用发起的调用由 useDebugCall 拿完整回包交过来（noteCallWording），
// 回包和事件谁先到都行：事件已经 reduce 过就直接补到那条上，还没到就先记着，reduce 到它时再取。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { errorText } from '../../core/domain/errors';
import {
    emptyChat,
    markSessionRead,
    reduceEvents,
    withCallWording,
    type CallWording,
    type ChatState,
    type SessionKey,
} from '../../core/domain/debug/chat';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugEvent } from '../../core/ipc/generated/debug/DebugEvent';
import type { DebugEventBatch } from '../../core/ipc/generated/debug/DebugEventBatch';
import type { DebugReceiverInfo } from '../../core/ipc/generated/debug/DebugReceiverInfo';
import type { DebugReceiverState } from '../../core/ipc/generated/debug/DebugReceiverState';
import { channelIdKey } from './keys';

export interface BotEventState {
    chat: ChatState;
    /** 右栏当前看的会话；'all' 是「全部」 */
    activeSession: SessionKey | 'all';
    receiver: DebugReceiverInfo | null;
    /** 当前有效的订阅；null 表示没在看（或后端已停），下次 ensureReceiving 会重新订 */
    subscriptionId: string | null;
    /**
     * 我们没接到、也再要不回来的条数（离开期间环形缓冲挤掉的）。
     * 不能用接收器的 first_seq - 1：seq 跨接收器重启连续编号，重启后那些「更早的」其实还在时间线上
     */
    unseenDropped: number;
    error?: string;
}

interface EventStoreState {
    bots: Record<string, BotEventState>;
}

const store = createStore<EventStoreState>({ bots: {} });

/** 窗口藏在后台时浏览器不出帧，队列会一直涨；到点也要 reduce 一次 */
const HIDDEN_FLUSH_MS = 500;
/** 等事件来取的 wording 最多记这么多条：事件一般几十毫秒内就到，积着没人取的是订阅断了或被清屏的 */
const MAX_PENDING_WORDINGS = 64;

interface Runtime {
    queue: DebugEvent[];
    scheduled: boolean;
    frame: number;
    fallback: ReturnType<typeof setTimeout> | null;
    /** 每次订阅 / 释放递增；旧订阅晚到的批次凭它认出来丢掉 */
    epoch: number;
    source: DebugChannelId | null;
    subscribing: Promise<void> | null;
    /** 不大于它的 receiver 事件是订阅之前的历史，不能再改接收器状态 */
    stateFloor: number;
    /**
     * 页面主动释放（releaseView / stopReceiving）的次数。排在别的订阅后面等着的 ensureReceiving
     * 靠它看「等的时候页面是不是已经走了」；换通道时内部的退订不算，否则后面排队的调用会被误伤
     */
    releases: number;
    /** request_id → 回包里的 wording，回包比事件先到时记在这里；取一次就删 */
    wordings: Map<string, string>;
    /** 顶栏「停止」留下的手动意图：之后任何自动路径（Bot 恢复运行等）都不再替用户订回来 */
    manualStop: boolean;
    /**
     * 手里还挂着订阅时接收器被后端停了（Bot 停止等）：原来就在看，Bot 恢复运行时自动补订。
     * 手动停止、空闲清扫（本来就没订阅）都不置上
     */
    resumeOnRunning: boolean;
}

const runtimes = new Map<string, Runtime>();

function runtimeOf(botId: string): Runtime {
    let rt = runtimes.get(botId);
    if (!rt) {
        rt = {
            queue: [],
            scheduled: false,
            frame: 0,
            fallback: null,
            epoch: 0,
            source: null,
            subscribing: null,
            stateFloor: 0,
            releases: 0,
            wordings: new Map(),
            manualStop: false,
            resumeOnRunning: false,
        };
        runtimes.set(botId, rt);
    }
    return rt;
}

function newEntry(selfId?: number): BotEventState {
    return {
        chat: emptyChat(selfId),
        activeSession: 'all',
        receiver: null,
        subscriptionId: null,
        unseenDropped: 0,
    };
}

function entryOf(botId: string): BotEventState | undefined {
    return store.getSnapshot().bots[botId];
}

function patchBot(botId: string, fn: (e: BotEventState) => BotEventState): void {
    const s = store.getSnapshot();
    const cur = s.bots[botId] ?? newEntry();
    const next = fn(cur);
    if (next === cur && s.bots[botId]) return;
    store.setState({ bots: { ...s.bots, [botId]: next } });
}

function unsubscribeQuietly(subscriptionId: string): void {
    // 退订只是通知后端少数一个观众；失败了后端的空闲超时也会收尾，不值得打扰用户
    onebotDebugService.unsubscribe(subscriptionId).catch((err) => {
        console.error('[debug] 退订事件流失败:', errorText(err));
    });
}

// ---------------------------------------------------------------------------
// 攒批 → 一帧一次
// ---------------------------------------------------------------------------

function cancelSchedule(rt: Runtime): void {
    if (!rt.scheduled) return;
    cancelAnimationFrame(rt.frame);
    if (rt.fallback !== null) clearTimeout(rt.fallback);
    rt.fallback = null;
    rt.scheduled = false;
}

function scheduleFlush(botId: string, rt: Runtime): void {
    if (rt.scheduled) return;
    rt.scheduled = true;
    const run = () => flushBot(botId);
    rt.frame = requestAnimationFrame(run);
    rt.fallback = setTimeout(run, HIDDEN_FLUSH_MS);
}

function onBatch(botId: string, epoch: number, batch: DebugEventBatch): void {
    // 空闲清扫的探活批次：没有内容，连帧都不排
    if (batch.events.length === 0) return;
    const rt = runtimes.get(botId);
    if (!rt || rt.epoch !== epoch || batch.bot_id !== botId) return;
    for (const e of batch.events) rt.queue.push(e);
    scheduleFlush(botId, rt);
}

/** 给 reduce 用的「取 wording」：没有记着的就不传，reduce 里连问都不用问 */
function wordingTaker(rt: Runtime): CallWording | undefined {
    if (rt.wordings.size === 0) return undefined;
    return (requestId) => {
        const wording = rt.wordings.get(requestId);
        if (wording !== undefined) rt.wordings.delete(requestId);
        return wording;
    };
}

function flushBot(botId: string): void {
    const rt = runtimes.get(botId);
    if (!rt) return;
    cancelSchedule(rt);
    if (rt.queue.length === 0) return;
    const events = rt.queue;
    rt.queue = [];

    // 接收器状态：只认订阅之后的 receiver 事件，同一批里取 seq 最大的
    let latestSeq = rt.stateFloor;
    let latest: DebugReceiverState | null = null;
    for (const e of events) {
        if (e.body.kind === 'receiver' && e.seq > latestSeq) {
            latestSeq = e.seq;
            latest = e.body.state;
        }
    }
    rt.stateFloor = latestSeq;

    const entry = entryOf(botId) ?? newEntry();
    const chat = reduceEvents(entry.chat, events, {
        activeSession: entry.activeSession,
        callWording: wordingTaker(rt),
    });
    let next = chat === entry.chat ? entry : { ...entry, chat };
    let stoppedSubscription: string | null = null;
    if (latest) {
        const state: DebugReceiverState = latest;
        if (next.receiver) next = { ...next, receiver: { ...next.receiver, state } };
        // 后端停了接收器（Bot 停止、手动停止、空闲超时）：手里的订阅已经没用，
        // 清掉它，页面下次 ensureReceiving 才会重新订
        if (state.state === 'stopped' && next.subscriptionId) {
            stoppedSubscription = next.subscriptionId;
            next = { ...next, subscriptionId: null };
            // 停的时候我们还盯着：Bot 要是恢复运行，替用户把接收自动续上
            rt.resumeOnRunning = true;
        }
    }
    patchBot(botId, () => next);
    if (stoppedSubscription) {
        rt.source = null;
        unsubscribeQuietly(stoppedSubscription);
    }
}

// ---------------------------------------------------------------------------
// 订阅生命周期
// ---------------------------------------------------------------------------

/** 缓冲里最后一条的 seq（缓冲连续，最早一条 + 条数 - 1） */
function ringLastSeq(info: DebugReceiverInfo): number {
    return info.buffered > 0 ? info.first_seq + info.buffered - 1 : 0;
}

/**
 * 开始收这个 Bot 的事件；已经在收（或正在订）同一条通道就什么也不做。
 * 换了事件通道会先退掉旧订阅再订新的。订阅失败不抛，写进该 Bot 的 error 并弹错误条。
 * 这是自动路径：页面效果、Bot 恢复运行的补订都走它；用户手动「停止」过就什么也不做，
 * 恢复要靠 restartReceiving（顶栏「重新接收」）
 */
function ensureReceiving(botId: string, source: DebugChannelId, selfId?: number): Promise<void> {
    const rt = runtimeOf(botId);
    if (rt.manualStop) return Promise.resolve();
    acquireEventBridge();
    const sameSource = rt.source !== null && channelIdKey(rt.source) === channelIdKey(source);

    if (rt.subscribing) {
        if (sameSource) return rt.subscribing;
        // 前一个订阅还在路上、这次要的是另一条通道：等它落定再换。等的时候页面走了（releaseView）就别再订了，
        // 否则页面已经离开，后端却多出一个没人看的观众
        const releasesAtCall = rt.releases;
        return rt.subscribing.then(() => {
            if (rt.releases !== releasesAtCall) return;
            return ensureReceiving(botId, source, selfId);
        });
    }
    if (entryOf(botId)?.subscriptionId) {
        if (sameSource) return Promise.resolve();
        dropSubscription(botId);
    }

    // 记下自己的 QQ 号：判断「自己发的」气泡靠它
    if (selfId !== undefined) {
        patchBot(botId, (e) =>
            e.chat.selfId === selfId
                ? e
                : { ...e, chat: { ...e.chat, selfId: e.chat.selfId ?? selfId } },
        );
    }

    rt.source = source;
    rt.epoch += 1;
    const attempt: Promise<void> = subscribeOnce(botId, rt, source, rt.epoch).finally(() => {
        if (rt.subscribing === attempt) rt.subscribing = null;
    });
    rt.subscribing = attempt;
    return attempt;
}

/**
 * 用户明确的「重新接收」：解除手动停止的标记再订。和 ensureReceiving 区分开，
 * 是为了手动停止之后自动路径（页面效果、Bot 恢复运行）不会悄悄把接收替用户订回来
 */
function restartReceiving(botId: string, source: DebugChannelId, selfId?: number): Promise<void> {
    const rt = runtimeOf(botId);
    rt.manualStop = false;
    return ensureReceiving(botId, source, selfId);
}

/** 订一次并把结果写进 store；不抛（失败写进 error 并弹条） */
async function subscribeOnce(
    botId: string,
    rt: Runtime,
    source: DebugChannelId,
    epoch: number,
): Promise<void> {
    // 出发时已经到手的最大 seq（时间线上的 + 排队等出帧的）：订阅回来后，缓冲起点之前的才算真丢
    let seenBefore = entryOf(botId)?.chat.lastSeq ?? 0;
    for (const e of rt.queue) seenBefore = Math.max(seenBefore, e.seq);
    try {
        const res = await onebotDebugService.subscribe(botId, source, (batch) =>
            onBatch(botId, epoch, batch),
        );
        if (rt.epoch !== epoch) {
            // 等回包的时候页面已经走了 / 又换了通道：这个订阅没人要，别让后端白白多一个观众
            unsubscribeQuietly(res.subscription_id);
            return;
        }
        // 补给我们的积压都不晚于这个位置；它们里头旧的 Stopped / Reconnecting 不该再改状态。
        // 不把等回包期间进队列的事件算进去：ringLastSeq 已盖住积压范围，而订阅途中到达的
        // 新接收器事件（比如恢复成 connected）必须照常生效，不然状态会卡在订阅那一刻
        const floor = Math.max(
            rt.stateFloor,
            entryOf(botId)?.chat.lastSeq ?? 0,
            ringLastSeq(res.receiver),
        );
        rt.stateFloor = floor;
        // 接收器重建只是接着编号，不算丢（旧条目还在时间线上）；
        // first_seq 比我们见过的更靠后，才是离开期间缓冲挤掉的、再也要不回来的
        const missed = Math.max(0, res.receiver.first_seq - 1 - seenBefore);
        patchBot(botId, (e) => ({
            ...e,
            subscriptionId: res.subscription_id,
            receiver: res.receiver,
            unseenDropped: e.unseenDropped + missed,
            error: undefined,
        }));
    } catch (err) {
        if (rt.epoch !== epoch) return;
        const raw = errorText(err);
        patchBot(botId, (e) => ({ ...e, error: raw }));
        rt.source = null;
        pushErrorBar({ key: `debug-receive:${botId}`, title: '无法开始接收事件', raw });
    }
}

/**
 * 页面不再看这个 Bot（切走 Bot、离开调试台）：退订，但后端接收器继续收，缓冲照写。
 * 队列里已到的先应用掉，免得回来时白白丢一段。
 */
function releaseView(botId: string): void {
    const rt = runtimes.get(botId);
    if (!rt) return;
    rt.releases += 1;
    dropSubscription(botId);
}

/** 退掉当前订阅（含在路上的），不算页面主动释放 */
function dropSubscription(botId: string): void {
    const rt = runtimes.get(botId);
    if (!rt) return;
    flushBot(botId);
    rt.epoch += 1;
    rt.subscribing = null;
    rt.source = null;
    const id = entryOf(botId)?.subscriptionId;
    if (id) {
        patchBot(botId, (e) => ({ ...e, subscriptionId: null }));
        unsubscribeQuietly(id);
    }
}

/** 让后端停止接收（顶栏「停止」）。失败弹错误条（保留订阅，手动标记也撤掉）；成功后本地订阅一并收掉 */
async function stopReceiving(botId: string): Promise<void> {
    const rt = runtimeOf(botId);
    rt.manualStop = true;
    rt.resumeOnRunning = false;
    try {
        await onebotDebugService.stopReceiver(botId);
    } catch (err) {
        rt.manualStop = false;
        pushErrorBar({
            key: `debug-stop-receiver:${botId}`,
            title: '停止接收失败',
            raw: errorText(err),
        });
        return;
    }
    releaseView(botId);
    // 后端会发一条 Stopped 事件，但订阅已经退了，等不到；本地直接标成已停
    patchBot(botId, (e) =>
        e.receiver && e.receiver.state.state !== 'stopped'
            ? {
                  ...e,
                  receiver: { ...e.receiver, state: { state: 'stopped', reason: '已手动停止' } },
              }
            : e,
    );
}

function setActiveSession(botId: string, key: SessionKey | 'all'): void {
    patchBot(botId, (e) => {
        const chat = key === 'all' ? e.chat : markSessionRead(e.chat, key);
        return e.activeSession === key && chat === e.chat ? e : { ...e, activeSession: key, chat };
    });
}

/**
 * 清屏：条目和会话清掉，但 lastSeq 保留，不然重新订阅补的积压会把旧内容原样补回来。
 * 记下的人名留着：清屏只是不想看旧消息，之后的通知照样该写人名而不是号
 */
function clearChat(botId: string): void {
    patchBot(botId, (e) => ({
        ...e,
        activeSession: 'all',
        chat: { ...emptyChat(e.chat.selfId), names: e.chat.names, lastSeq: e.chat.lastSeq },
    }));
}

/**
 * 本应用发起的调用拿到了回包、但 OB11 说失败：把上游的说明（wording / message）挂到时间线上那条调用上。
 * 那条调用的事件可能已经 reduce 过（直接补），也可能还在路上或在队列里等下一帧（先记着，reduce 时取）。
 */
function noteCallWording(botId: string, requestId: string, wording: string): void {
    const text = wording.trim();
    if (!text) return;
    const entry = entryOf(botId);
    if (entry) {
        const chat = withCallWording(entry.chat, requestId, text);
        if (chat !== entry.chat) {
            patchBot(botId, (e) => ({ ...e, chat }));
            return;
        }
    }
    const pending = runtimeOf(botId).wordings;
    pending.delete(requestId);
    pending.set(requestId, text);
    if (pending.size > MAX_PENDING_WORDINGS) {
        const oldest = pending.keys().next().value;
        if (oldest !== undefined) pending.delete(oldest);
    }
}

// ---------------------------------------------------------------------------
// Bot 恢复运行时补订
// ---------------------------------------------------------------------------

let eventBridgeStarted = false;

/**
 * 接收器被后端停了（Bot 停止等）之后，单靠页面 useEffect 观察 running 翻转来补订不可靠：
 * 重启够快时 targets 的重拉会被后一次失效顶掉，页面根本没看见 false → true。
 * 域事件是推送制，一次都漏不掉，所以 bridge 放在模块级，跟着第一次订阅建立，之后一直收着。
 */
function acquireEventBridge(): void {
    if (eventBridgeStarted) return;
    eventBridgeStarted = true;
    subscribeDomainEvents((event) => {
        if (event.kind === 'bot_state_changed' && event.snapshot.state === 'running') {
            resumeAfterBotRestart(event.snapshot.bot_id);
        }
    });
}

function resumeAfterBotRestart(botId: string): void {
    const rt = runtimes.get(botId);
    const entry = entryOf(botId);
    // 只替「停的时候还在看」的 Bot 补订；手动停止、本来就没订过（离开了页面）都不替用户做主
    if (!rt || !entry || !rt.resumeOnRunning || rt.manualStop) return;
    if (entry.subscriptionId || !entry.receiver) return;
    rt.resumeOnRunning = false;
    void ensureReceiving(botId, entry.receiver.source);
}

export const debugEventStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,
    ensureReceiving,
    restartReceiving,
    releaseView,
    stopReceiving,
    setActiveSession,
    clearChat,
    noteCallWording,

    // 测试 / dev 重置用。
    _reset(): void {
        for (const rt of runtimes.values()) cancelSchedule(rt);
        runtimes.clear();
        receiverViews.clear();
        store._reset();
    },
};

// ---------------------------------------------------------------------------
// React
// ---------------------------------------------------------------------------

const EMPTY_CHAT = emptyChat();

/** 没订过的 Bot 也返回同一份空聊天，避免每次渲染新对象 */
export function useDebugChat(botId: string | null): ChatState {
    return useSyncExternalStore(
        store.subscribe,
        () => (botId ? entryOf(botId)?.chat : undefined) ?? EMPTY_CHAT,
    );
}

export function useDebugActiveSession(botId: string | null): SessionKey | 'all' {
    return useSyncExternalStore(
        store.subscribe,
        () => (botId ? entryOf(botId)?.activeSession : undefined) ?? 'all',
    );
}

export interface DebugReceiverView {
    receiver: DebugReceiverInfo | null;
    /** 最新的接收器状态（订阅返回的，之后由事件流更新）；没订过是 null */
    state: DebugReceiverState | null;
    /** 现在是否挂着一个有效订阅 */
    subscribed: boolean;
    /** 我们没接到、也再要不回来的事件条数（离开期间缓冲挤掉的），时间线顶部的「已丢弃」用它 */
    unseenDropped: number;
    error: string | null;
}

const IDLE_VIEW: DebugReceiverView = {
    receiver: null,
    state: null,
    subscribed: false,
    unseenDropped: 0,
    error: null,
};

// chat 每帧都在变，视图对象却只该在这几项变了才换，否则订阅它的组件会跟着聊天一起刷
const receiverViews = new Map<
    string,
    {
        receiver: DebugReceiverInfo | null;
        subscriptionId: string | null;
        unseenDropped: number;
        error: string | null;
        view: DebugReceiverView;
    }
>();

function receiverViewOf(botId: string | null): DebugReceiverView {
    const entry = botId ? entryOf(botId) : undefined;
    if (!botId || !entry) return IDLE_VIEW;
    const error = entry.error ?? null;
    const cached = receiverViews.get(botId);
    if (
        cached &&
        cached.receiver === entry.receiver &&
        cached.subscriptionId === entry.subscriptionId &&
        cached.unseenDropped === entry.unseenDropped &&
        cached.error === error
    ) {
        return cached.view;
    }
    const view: DebugReceiverView = {
        receiver: entry.receiver,
        state: entry.receiver?.state ?? null,
        subscribed: entry.subscriptionId !== null,
        unseenDropped: entry.unseenDropped,
        error,
    };
    receiverViews.set(botId, {
        receiver: entry.receiver,
        subscriptionId: entry.subscriptionId,
        unseenDropped: entry.unseenDropped,
        error,
        view,
    });
    return view;
}

export function useDebugReceiverState(botId: string | null): DebugReceiverView {
    return useSyncExternalStore(store.subscribe, () => receiverViewOf(botId));
}
