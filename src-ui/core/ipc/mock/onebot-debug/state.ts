// 每个 Bot 的运行期状态与事件流引擎：环形缓冲、订阅批次、接收器定时器；
// 机器人页 Bot 启停联动事件流的接线也在这（原来紧跟着 BOTS 合并循环；回调只在事件
// 到达时跑，那时 states 早已初始化好，前后挪不影响行为）。

import type { DebugChannelId } from '../../generated/debug/DebugChannelId';
import type { DebugEvent } from '../../generated/debug/DebugEvent';
import type { DebugEventBatch } from '../../generated/debug/DebugEventBatch';
import type { DebugEventBody } from '../../generated/debug/DebugEventBody';
import type { DebugReceiverInfo } from '../../generated/debug/DebugReceiverInfo';
import { subscribeMockEvents } from '../events.mock';
import { mockBots as pageBots } from '../bot.mock';
import {
    mulberry32,
    seedFromString,
    randInt,
    makeGroupMessage,
    makeHeartbeat,
    makeLifecycle,
    makeRandomEvent,
    type EventContext,
    type Rng,
} from '../onebot-debug-events.mock';
import { BOTS, channelKey, type MockBot } from './bots';
import {
    EVENT_VERSION,
    RING_CAP,
    RING_TRIM_SLACK,
    BATCH_FLUSH_MS,
    HEARTBEAT_EVERY_MS,
    PROBE_EVERY_MS,
    MESSAGE_STORE_CAP,
    FIRST_MESSAGE_ID,
    type Timer,
} from './consts';

// ---------------------------------------------------------------------------
// 每个 Bot 的运行期状态：事件缓冲、消息库、接收器
// ---------------------------------------------------------------------------

export interface Subscriber {
    id: string;
    onBatch: (batch: DebugEventBatch) => void;
    queue: DebugEvent[];
    flushTimer: Timer | null;
}

export interface Receiver {
    source: DebugChannelId;
    subs: Map<string, Subscriber>;
    eventTimer: Timer | null;
    heartbeatTimer: Timer | null;
    probeTimer: Timer | null;
    /** 最后一个订阅者走开的时间；下次有人订阅时按这段时长补事件 */
    pausedAt: number | null;
}

export interface BotState {
    bot: MockBot;
    seq: number;
    ring: DebugEvent[];
    droppedTotal: number;
    messages: Map<number, Record<string, unknown>>;
    recentGroup: number[];
    rng: Rng;
    ctx: EventContext;
    receiver: Receiver | null;
    /** 第一批假历史只补一次：接收器重建（停止 → 重新接收）不再补带旧时间戳的事件 */
    seeded: boolean;
}

export const states = new Map<string, BotState>();
/** 订阅号 → Bot，退订只给订阅号 */
export const subscriptions = new Map<string, string>();
export let messageSeq = 0;
export let subscriptionSeq = 0;

export const nextMessageId = () => FIRST_MESSAGE_ID + ++messageSeq;

export function stateOf(bot: MockBot): BotState {
    let st = states.get(bot.id);
    if (!st) {
        const rng = mulberry32(seedFromString(bot.id));
        const created: BotState = {
            bot,
            seq: 0,
            ring: [],
            droppedTotal: 0,
            messages: new Map(),
            recentGroup: [],
            rng,
            receiver: null,
            seeded: false,
            ctx: {
                backend: bot.backend,
                selfId: bot.qq,
                selfName: bot.name,
                rng,
                nextMessageId,
                recentGroupMessageIds: () => created.recentGroup,
            },
        };
        states.set(bot.id, created);
        st = created;
    }
    return st;
}

export function rememberMessage(st: BotState, payload: Record<string, unknown>): void {
    const id = payload.message_id;
    if (typeof id !== 'number') return;
    st.messages.set(id, payload);
    if (st.messages.size > MESSAGE_STORE_CAP) {
        const oldest = st.messages.keys().next();
        if (!oldest.done) st.messages.delete(oldest.value);
    }
    if (payload.post_type === 'message' && payload.message_type === 'group') {
        st.recentGroup.push(id);
        if (st.recentGroup.length > 30) st.recentGroup.shift();
    }
}

export function flushNow(st: BotState, sub: Subscriber): void {
    if (sub.flushTimer !== null) {
        clearTimeout(sub.flushTimer);
        sub.flushTimer = null;
    }
    if (sub.queue.length === 0) return;
    const events = sub.queue;
    sub.queue = [];
    sub.onBatch({ v: EVENT_VERSION, bot_id: st.bot.id, events });
}

export function scheduleFlush(st: BotState, sub: Subscriber): void {
    if (sub.flushTimer !== null) return;
    sub.flushTimer = setTimeout(() => {
        sub.flushTimer = null;
        flushNow(st, sub);
    }, BATCH_FLUSH_MS);
}

export function trimRing(st: BotState): void {
    if (st.ring.length < RING_CAP + RING_TRIM_SLACK) return;
    const drop = st.ring.length - RING_CAP;
    st.ring.splice(0, drop);
    st.droppedTotal += drop;
    pushBodyAt(st, { kind: 'dropped', count: drop }, Date.now());
}

/** 写进缓冲并推给所有订阅者；这个 Bot 没有接收器就什么都不记（后端也是不收的） */
export function pushBodyAt(st: BotState, body: DebugEventBody, atMs: number): void {
    const rc = st.receiver;
    if (!rc) return;
    st.seq += 1;
    const event: DebugEvent = { seq: st.seq, at_ms: atMs, body };
    st.ring.push(event);
    for (const sub of rc.subs.values()) {
        sub.queue.push(event);
        scheduleFlush(st, sub);
    }
    if (body.kind !== 'dropped') trimRing(st);
}

export const pushBody = (st: BotState, body: DebugEventBody) => pushBodyAt(st, body, Date.now());

export function pushOb11At(st: BotState, payload: Record<string, unknown>, atMs: number): void {
    if (payload.post_type === 'message' || payload.post_type === 'message_sent')
        rememberMessage(st, payload);
    pushBodyAt(st, { kind: 'ob11', payload }, atMs);
}

export const pushOb11 = (st: BotState, payload: Record<string, unknown>) =>
    pushOb11At(st, payload, Date.now());

// —— 接收器与定时器

export function stopHeartbeat(rc: Receiver): void {
    if (rc.heartbeatTimer !== null) clearTimeout(rc.heartbeatTimer);
    rc.heartbeatTimer = null;
}

export function stopTimers(rc: Receiver): void {
    if (rc.eventTimer !== null) clearTimeout(rc.eventTimer);
    if (rc.probeTimer !== null) clearTimeout(rc.probeTimer);
    stopHeartbeat(rc);
    rc.eventTimer = null;
    rc.probeTimer = null;
    rc.pausedAt = Date.now();
}

/**
 * NapCat 的内部通道是 WebUI 的调试 WS：上游把每个 OB11 事件都转过来，唯独不发心跳。
 * 预览在这条来源上也不冒心跳，界面才会碰到「长时间没有心跳」这种真实情形；其它来源照旧
 */
export const sendsHeartbeat = (st: BotState, rc: Receiver): boolean =>
    !(st.bot.backend === 'napcat' && rc.source.kind === 'internal');

export function scheduleNextEvent(st: BotState, rc: Receiver): void {
    rc.eventTimer = setTimeout(
        () => {
            rc.eventTimer = null;
            pushOb11(st, makeRandomEvent(st.ctx, Date.now()));
            scheduleNextEvent(st, rc);
        },
        randInt(st.rng, 1200, 3000),
    );
}

export function scheduleNextHeartbeat(st: BotState, rc: Receiver): void {
    rc.heartbeatTimer = setTimeout(() => {
        rc.heartbeatTimer = null;
        pushOb11(st, makeHeartbeat(st.ctx, Date.now()));
        scheduleNextHeartbeat(st, rc);
    }, HEARTBEAT_EVERY_MS);
}

/**
 * 后端的清扫每分钟给每个窗口发一个空批次（`events: []`），把已经关掉的窗口摘掉。
 * 预览照样发，界面在浏览器里也要走到「收到空批次」这条路
 */
export function scheduleProbe(st: BotState, rc: Receiver): void {
    rc.probeTimer = setTimeout(() => {
        rc.probeTimer = null;
        for (const sub of [...rc.subs.values()])
            sub.onBatch({ v: EVENT_VERSION, bot_id: st.bot.id, events: [] });
        // 回调里可能已经退订到零或停了接收器，这时不能再挂下一轮
        if (st.receiver === rc && rc.subs.size > 0 && rc.probeTimer === null) scheduleProbe(st, rc);
    }, PROBE_EVERY_MS);
}

/** 每次有人订阅都走这里；换过事件来源时心跳要跟着开或关 */
export function startTimers(st: BotState, rc: Receiver): void {
    if (rc.eventTimer === null) scheduleNextEvent(st, rc);
    if (!sendsHeartbeat(st, rc)) stopHeartbeat(rc);
    else if (rc.heartbeatTimer === null) scheduleNextHeartbeat(st, rc);
    if (rc.probeTimer === null) scheduleProbe(st, rc);
    rc.pausedAt = null;
}

/** 没人订阅的这段时间里「后端」还在收，补一批带真实时间戳的事件，最多 100 条 */
export function catchUp(st: BotState, from: number, to: number): void {
    let at = from;
    for (let n = 0; n < 100; n += 1) {
        at += randInt(st.rng, 1200, 3000);
        if (at >= to) break;
        pushOb11At(st, makeRandomEvent(st.ctx, at), at);
    }
}

export function seedReceiver(st: BotState, source: DebugChannelId): void {
    const now = Date.now();
    if (st.seeded) {
        // 重建的接收器（顶栏「停止 → 重新接收」）只是又一次上线：事件接着往后编，
        // 不补带旧时间戳的假历史，不然时间线末尾会接上一段比已有条目更早的事件
        pushBody(st, { kind: 'receiver', state: { state: 'connecting' }, source });
        pushBody(st, { kind: 'receiver', state: { state: 'connected' }, source });
        return;
    }
    st.seeded = true;
    pushBodyAt(st, { kind: 'receiver', state: { state: 'connecting' }, source }, now - 60_000);
    pushBodyAt(st, { kind: 'receiver', state: { state: 'connected' }, source }, now - 59_000);
    pushOb11At(st, makeLifecycle(st.ctx, now - 58_000), now - 58_000);
    for (let i = 0; i < 20; i += 1) {
        const at = now - 55_000 + i * 2_500;
        pushOb11At(st, makeRandomEvent(st.ctx, at), at);
    }
}

export function ensureReceiver(st: BotState, source: DebugChannelId): Receiver {
    if (!st.receiver) {
        st.receiver = {
            source,
            subs: new Map(),
            eventTimer: null,
            heartbeatTimer: null,
            probeTimer: null,
            pausedAt: null,
        };
        seedReceiver(st, source);
    } else if (channelKey(st.receiver.source) !== channelKey(source)) {
        // 换事件来源：缓冲保留，接收器重连
        st.receiver.source = source;
        pushBody(st, { kind: 'receiver', state: { state: 'connecting' }, source });
        pushBody(st, { kind: 'receiver', state: { state: 'connected' }, source });
    }
    return st.receiver;
}

export function receiverInfo(st: BotState): DebugReceiverInfo | null {
    const rc = st.receiver;
    if (!rc) return null;
    return {
        bot_id: st.bot.id,
        source: rc.source,
        state: { state: 'connected' },
        buffered: st.ring.length,
        dropped_total: st.droppedTotal,
        first_seq: st.ring.length > 0 ? st.ring[0].seq : st.seq + 1,
        viewers: rc.subs.size,
    };
}

export function shutdownReceiver(st: BotState, reason: string): void {
    const rc = st.receiver;
    if (!rc) return;
    pushBody(st, { kind: 'receiver', state: { state: 'stopped', reason }, source: rc.source });
    for (const sub of rc.subs.values()) {
        flushNow(st, sub);
        subscriptions.delete(sub.id);
    }
    rc.subs.clear();
    stopTimers(rc);
    st.receiver = null;
    st.ring = [];
}

/** 浏览器控制台压测：往正在被看的 Bot 里一口气灌 n 条群消息，返回实际灌了多少 */
export function flood(n: number, botId?: string): number {
    let pushed = 0;
    for (const st of states.values()) {
        if (botId && st.bot.id !== botId) continue;
        if (!st.receiver || st.receiver.subs.size === 0) continue;
        for (let i = 0; i < n; i += 1) pushOb11(st, makeGroupMessage(st.ctx, Date.now()));
        pushed += n;
    }
    return pushed;
}

subscribeMockEvents((event) => {
    if (event.kind !== 'bot_state_changed') return;
    const bot = BOTS.find((b) => b.id === event.snapshot.bot_id);
    if (!bot || !pageBots.some((p) => p.bot_id === bot.id)) return;
    const running = event.snapshot.state === 'running';
    if (bot.running === running) return;
    bot.running = running;
    bot.online = running ? true : null;
    if (!running) {
        const st = states.get(bot.id);
        if (st?.receiver) shutdownReceiver(st, 'Bot 已停止');
    }
});

/** 订阅号自增；onebotDebugMock.subscribe 建订阅时取号 */
export const nextSubscriptionId = (): number => ++subscriptionSeq;

/** 清掉所有接收器、定时器、订阅与事件计数器；Bot 状态连同各自种过子的 rng 一起重建 */
export function resetEventMock(): void {
    for (const st of states.values()) {
        const rc = st.receiver;
        if (!rc) continue;
        for (const sub of rc.subs.values())
            if (sub.flushTimer !== null) clearTimeout(sub.flushTimer);
        rc.subs.clear();
        stopTimers(rc);
    }
    states.clear();
    subscriptions.clear();
    messageSeq = 0;
    subscriptionSeq = 0;
}
