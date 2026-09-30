// 调试台的浏览器预览假后端：三个 Bot、各自的通道、接口目录、能回数据的调用、会自己冒事件的事件流。
// 真 IPC 在 `core/services/onebot-debug.service.ts`。
//
// 全部状态只在内存里，刷新页面即重置；随机数都有固定种子，同样的操作序列得到同样的数据。
// 事件流的定时器只在有窗口订阅时才跑，退订到零就全部清掉，测试和 HMR 都不会留尾巴：
// 没人看的这段时间里「后端」照样在收，下次订阅时按缺的时长补一批事件，行为上和真后端一致。

import type { BackendType } from '../generated/domain/BackendType';
import type { DebugCallOutcome } from '../generated/debug/DebugCallOutcome';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../generated/debug/DebugCallResponse';
import type { DebugCallResult } from '../generated/debug/DebugCallResult';
import type { DebugCatalog } from '../generated/debug/DebugCatalog';
import type { DebugChannelId } from '../generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../generated/debug/DebugChannelInfo';
import type { DebugChannelStatus } from '../generated/debug/DebugChannelStatus';
import type { DebugChannels } from '../generated/debug/DebugChannels';
import type { DebugCollections } from '../generated/debug/DebugCollections';
import type { DebugError } from '../generated/debug/DebugError';
import type { DebugEvent } from '../generated/debug/DebugEvent';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';
import type { DebugEventBody } from '../generated/debug/DebugEventBody';
import type { DebugHistoryEntry } from '../generated/debug/DebugHistoryEntry';
import type { DebugHistoryPage } from '../generated/debug/DebugHistoryPage';
import type { DebugHistoryQuery } from '../generated/debug/DebugHistoryQuery';
import type { DebugHost } from '../generated/debug/DebugHost';
import type { DebugReceiverInfo } from '../generated/debug/DebugReceiverInfo';
import type { DebugStorageNotice } from '../generated/debug/DebugStorageNotice';
import type { DebugSubscribeResponse } from '../generated/debug/DebugSubscribeResponse';
import type { DebugTarget } from '../generated/debug/DebugTarget';
import type { DebugWorkspace } from '../generated/debug/DebugWorkspace';
import type { DebugActionSpec } from '../generated/debug/DebugActionSpec';
import { isTauri } from '../transport';
import { withMockDelay } from './bootstrap.mock';
import { mockBots as pageBots } from './bot.mock';
import { subscribeMockEvents } from './events.mock';
import { buildMockCatalog, buildMockSpec, mockRequiredParams, type MockSpecOptions } from './onebot-debug-catalog.mock';
import {
    BIG_GROUP_MEMBER_COUNT,
    FRIEND_COUNT,
    GROUP_COUNT,
    GROUP_ID_BASE,
    USER_ID_BASE,
    groupNameOf,
    isKnownGroup,
    makeActionNotice,
    makeGroupMessage,
    makeHeartbeat,
    makeLifecycle,
    makeRandomEvent,
    makeSelfMessage,
    memberCountOf,
    mulberry32,
    personAt,
    placeholderImage,
    randInt,
    seedFromString,
    messageToSegments,
    type EventContext,
    type MockPerson,
    type Ob11Segment,
    type Rng,
} from './onebot-debug-events.mock';

const EVENT_VERSION = 1;
/** 每个 Bot 的事件缓冲上限，和后端一致 */
const RING_CAP = 5000;
/** 超出上限多少条才裁一次：一次裁一批并写一条 dropped，不是每来一条就写 */
const RING_TRIM_SLACK = 100;
const BACKLOG_CHUNK = 500;
const BATCH_FLUSH_MS = 50;
const DEFAULT_TIMEOUT_MS = 60_000;
const MEMBER_LIST_DELAY_MS = 900;
/** 连通测试要走一趟上游，这么久之后才出结果 */
const TEST_CHANNEL_MS = 400;
const HEARTBEAT_EVERY_MS = 30_000;
/** 后端清扫的间隔：每一轮给每个窗口发一个空批次，探窗口还在不在 */
const PROBE_EVERY_MS = 60_000;
/** 回包文本超过这么大就不整块给界面，和后端 RESPONSE_INLINE_LIMIT 一致 */
const RESPONSE_INLINE_LIMIT = 5 * 1024 * 1024;
/** 截断时给界面的文本预览字节数，和后端 RAW_PREVIEW_LIMIT 一致 */
const RAW_PREVIEW_LIMIT = 256 * 1024;
/** 单条历史里回包序列化后的上限，超了回包整个不存、只留「被截断」标记，和后端一致 */
const HISTORY_RESPONSE_LIMIT = 256 * 1024;
/** 被截断的回包全文只留最近这么多次，够「另存完整内容」用，和后端一致 */
const LARGE_RESPONSES_KEPT = 3;
/** 「超大回包」动作的行数：每行一百多字节，合起来约 5.7 MiB，稳稳越过 5 MiB 的内联上限 */
const HUGE_RESPONSE_ROWS = 50_000;
const MESSAGE_STORE_CAP = 3000;
const HISTORY_CAP = 1000;
/**
 * 回包里是登录凭据的动作：后端的历史只记「调过」、不记回包（`_async` 变体同理），
 * 预览照做，免得界面在预览里显示出真程序永远不会有的回包
 */
const CREDENTIAL_ACTIONS: ReadonlySet<string> = new Set([
    'get_cookies',
    'get_credentials',
    'get_csrf_token',
    'get_clientkey',
    'get_rkey',
    'nc_get_rkey',
    'get_rkey_server',
]);
/** 收藏夹种子的时间戳：固定值，不让预览数据随打开时间飘 */
const SEED_EPOCH_MS = 1_759_190_400_000;
const FIRST_MESSAGE_ID = 1_700_000_000;

type Timer = ReturnType<typeof setTimeout>;

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

// ---------------------------------------------------------------------------
// Bot 与通道
// ---------------------------------------------------------------------------

interface MockBot {
    id: string;
    name: string;
    qq: number;
    backend: BackendType;
    host: DebugHost;
    running: boolean;
    online: boolean | null;
    /** 远端 / Docker 时桌面端开的本地隧道口 */
    tunnelPort: number;
    /** 远端 HTTP 服务的令牌是错的：用来走查「token 错误」这条路 */
    httpRejectsToken: boolean;
    /** 这个 Bot 的上游版本缺的动作，进目录的「当前 Bot 不支持」 */
    missingActions: ReadonlySet<string>;
    /** 通道令牌的打码结果 */
    tokenHint: string;
    /** 容器里没映射的 WS 端口，通道列表里标「暂不支持」 */
    unmappedWsPort?: number;
}

const BOTS: MockBot[] = [
    {
        id: 'mock-bot-sl',
        name: '小雪',
        qq: 2854196310,
        backend: 'snowluma',
        host: { kind: 'local' },
        running: true,
        online: true,
        tunnelPort: 0,
        httpRejectsToken: false,
        missingActions: new Set(),
        tokenHint: 'sn***a1',
    },
    {
        id: 'mock-bot-nc-remote',
        name: 'NapCat 测试号',
        qq: 1919810,
        backend: 'napcat',
        host: { kind: 'remote', server_id: 'srv-1' },
        running: true,
        online: true,
        tunnelPort: 54711,
        httpRejectsToken: true,
        missingActions: new Set(['fetch_custom_face']),
        tokenHint: 'nc***01',
    },
    {
        id: 'mock-bot-nc-docker',
        name: '容器里的 NC',
        qq: 3141592,
        backend: 'napcat',
        host: { kind: 'docker', server_id: 'srv-1' },
        running: false,
        online: null,
        tunnelPort: 54722,
        httpRejectsToken: false,
        missingActions: new Set(),
        tokenHint: 'dk***7f',
        unmappedWsPort: 8080,
    },
];

// 机器人页预览里的 Bot（10001 等）也出现在调试台的目标里：Bot 卡片的「调试」能跳进来，
// 启停它们时这边的 running 跟着变（停掉时接收器也收掉，时间线显示「Bot 已停止」）。
// 上面的三个固定 Bot 是调试台专用的走查场景（SL 本机 / NC 远端 / NC Docker），不并
for (const status of pageBots) {
    if (BOTS.some((b) => b.id === status.bot_id)) continue;
    BOTS.push({
        id: status.bot_id,
        name: `Bot-${status.bot_id.slice(-2)}`,
        qq: Number(status.bot_id),
        backend: 'napcat',
        host: { kind: 'local' },
        running: status.state === 'running',
        online: status.state === 'running' ? true : null,
        tunnelPort: 0,
        httpRejectsToken: false,
        missingActions: new Set(),
        tokenHint: 'pg***01',
    });
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

const findBot = (botId: string): MockBot | undefined => BOTS.find((b) => b.id === botId);

interface ChannelDef {
    id: DebugChannelId;
    label: string;
    canCall: boolean;
    canReceive: boolean;
    port: number | null;
    unsupported?: string;
}

function channelDefs(bot: MockBot): ChannelDef[] {
    const defs: ChannelDef[] = [
        {
            id: { kind: 'internal' },
            label: bot.backend === 'napcat' ? 'NapCat 内部通道' : 'SnowLuma 内部通道',
            canCall: true,
            canReceive: true,
            port: bot.backend === 'napcat' ? 6099 : 8090,
        },
        {
            id: { kind: 'http', name: 'http-default' },
            label: 'HTTP · http-default :3000',
            canCall: true,
            canReceive: false,
            port: 3000,
        },
        {
            id: { kind: 'ws', name: 'ws-default' },
            label: 'WS · ws-default :3001',
            canCall: true,
            canReceive: true,
            port: 3001,
        },
    ];
    if (bot.unmappedWsPort) {
        defs.push({
            id: { kind: 'ws', name: `ws-${bot.unmappedWsPort}` },
            label: `WS · ws-${bot.unmappedWsPort} :${bot.unmappedWsPort}`,
            canCall: true,
            canReceive: true,
            port: bot.unmappedWsPort,
            unsupported: `容器没有映射 ${bot.unmappedWsPort} 端口`,
        });
    }
    return defs;
}

const channelKey = (id: DebugChannelId): string => (id.kind === 'http' || id.kind === 'ws' ? `${id.kind}:${id.name}` : id.kind);

/** 测试连通 / 调用失败后记下的状态，覆盖初始状态 */
const statusOverrides = new Map<string, DebugChannelStatus>();
const overrideKey = (botId: string, id: DebugChannelId) => `${botId}|${channelKey(id)}`;
/** 每重置一次加一：重置之前发起、之后才测完的连通测试据此作废，不把旧结果写进新的状态 */
let mockGeneration = 0;

function initialStatus(bot: MockBot, def: ChannelDef): DebugChannelStatus {
    if (def.unsupported) return { kind: 'unsupported', reason: def.unsupported };
    if (!bot.running) return { kind: 'bot_not_running' };
    if (def.id.kind === 'ws') return { kind: 'unknown' };
    if (def.id.kind === 'http' && bot.host.kind !== 'local') return { kind: 'tunneled', local_port: bot.tunnelPort };
    return { kind: 'available' };
}

const currentStatus = (bot: MockBot, def: ChannelDef): DebugChannelStatus =>
    statusOverrides.get(overrideKey(bot.id, def.id)) ?? initialStatus(bot, def);

function endpointOf(bot: MockBot, def: ChannelDef): string | null {
    if (def.unsupported || def.port === null) return null;
    const tail = def.id.kind === 'internal' ? '/api' : '/';
    return bot.host.kind === 'local' ? `127.0.0.1:${def.port}${tail}` : `隧道 → 远端 127.0.0.1:${def.port}`;
}

function channelInfo(bot: MockBot, def: ChannelDef): DebugChannelInfo {
    return {
        id: def.id,
        label: def.label,
        can_call: def.canCall,
        can_receive: def.canReceive,
        status: currentStatus(bot, def),
        endpoint: endpointOf(bot, def),
        // 内部通道用的是 WebUI 登录态，没有单独的令牌
        token_hint: def.id.kind === 'internal' ? null : bot.tokenHint,
    };
}

const usable = (s: DebugChannelStatus) => s.kind === 'available' || s.kind === 'tunneled' || s.kind === 'unknown';

/** 「自动」落到哪条：先内部，再 WS，最后 HTTP；实测可用的排在没测过的前面 */
function pickAuto(bot: MockBot, purpose: 'call' | 'events'): DebugChannelId | null {
    const order = purpose === 'call' ? ['internal', 'ws', 'http'] : ['internal', 'ws'];
    const infos = channelDefs(bot)
        .map((d) => ({ d, status: currentStatus(bot, d) }))
        .filter(({ d, status }) => usable(status) && (purpose === 'call' ? d.canCall : d.canReceive));
    for (const wantKnown of [true, false]) {
        for (const kind of order) {
            const hit = infos.find(({ d, status }) => d.id.kind === kind && (status.kind !== 'unknown') === wantKnown);
            if (hit) return hit.d.id;
        }
    }
    return null;
}

function statusError(s: DebugChannelStatus): DebugError | null {
    switch (s.kind) {
        case 'unknown':
        case 'available':
        case 'tunneled':
            return null;
        case 'unreachable':
        case 'unsupported':
            return { kind: 'channel_unavailable', reason: s.reason };
        case 'auth_failed':
            return { kind: 'auth_failed', status: s.status };
        case 'upstream_too_old':
            return { kind: 'upstream_too_old' };
        case 'bot_not_running':
            return { kind: 'bot_not_running' };
        case 'not_logged_in':
            return { kind: 'not_logged_in' };
    }
}

type Resolved = { ok: true; def: ChannelDef } | { ok: false; error: DebugError };

function resolveChannel(bot: MockBot, id: DebugChannelId, purpose: 'call' | 'events'): Resolved {
    if (!bot.running) return { ok: false, error: { kind: 'bot_not_running' } };
    const target = id.kind === 'auto' ? pickAuto(bot, purpose) : id;
    if (!target) return { ok: false, error: { kind: 'channel_unavailable', reason: '没有可用的通道' } };
    const def = channelDefs(bot).find((d) => channelKey(d.id) === channelKey(target));
    if (!def) return { ok: false, error: { kind: 'channel_unavailable', reason: '这个 Bot 没有这条通道' } };
    if (purpose === 'events' && !def.canReceive) {
        return { ok: false, error: { kind: 'channel_unavailable', reason: '这条通道不能接收事件' } };
    }
    if (purpose === 'call' && !def.canCall) {
        return { ok: false, error: { kind: 'channel_unavailable', reason: '这条通道不能发起调用' } };
    }
    let status = currentStatus(bot, def);
    // 令牌错的 HTTP 服务：隧道能通，真正调用时才被上游拒绝，记下来后面的状态就一直是「token 错误」
    if (status.kind === 'tunneled' && def.id.kind === 'http' && bot.httpRejectsToken) {
        status = { kind: 'auth_failed', status: 401 };
        statusOverrides.set(overrideKey(bot.id, def.id), status);
    }
    const error = statusError(status);
    return error ? { ok: false, error } : { ok: true, def };
}

function errorText(error: DebugError): string {
    switch (error.kind) {
        case 'bot_not_found':
            return 'Bot 不存在';
        case 'bot_not_running':
            return 'Bot 没有在运行';
        case 'not_logged_in':
            return 'QQ 还没登录';
        case 'channel_unavailable':
            return `通道不可用：${error.reason}`;
        case 'upstream_too_old':
            return '上游版本太老，没有调试接口';
        case 'auth_failed':
            return `鉴权失败（HTTP ${error.status}）`;
        case 'timeout':
            return `等待超过 ${error.ms} 毫秒`;
        case 'cancelled':
            return '已取消';
        case 'transport':
        case 'invalid_params':
        case 'internal':
            return error.message;
        case 'feature_disabled':
            return '调试台已关闭';
    }
}

// ---------------------------------------------------------------------------
// 随机数与延迟
// ---------------------------------------------------------------------------

let delayRng: Rng = mulberry32(0xd31a);
let callRng: Rng = mulberry32(0xca11);

/** 普通读写的模拟延迟：80–300 ms */
const respond = <T>(value: T): Promise<T> => withMockDelay(value, randInt(delayRng, 80, 300));
const rejectAfterDelay = (reason: string): Promise<never> =>
    new Promise((_, reject) => setTimeout(() => reject(reason), randInt(delayRng, 80, 300)));

// ---------------------------------------------------------------------------
// 每个 Bot 的运行期状态：事件缓冲、消息库、接收器
// ---------------------------------------------------------------------------

interface Subscriber {
    id: string;
    onBatch: (batch: DebugEventBatch) => void;
    queue: DebugEvent[];
    flushTimer: Timer | null;
}

interface Receiver {
    source: DebugChannelId;
    subs: Map<string, Subscriber>;
    eventTimer: Timer | null;
    heartbeatTimer: Timer | null;
    probeTimer: Timer | null;
    /** 最后一个订阅者走开的时间；下次有人订阅时按这段时长补事件 */
    pausedAt: number | null;
}

interface BotState {
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

const states = new Map<string, BotState>();
/** 订阅号 → Bot，退订只给订阅号 */
const subscriptions = new Map<string, string>();
let messageSeq = 0;
let subscriptionSeq = 0;

const nextMessageId = () => FIRST_MESSAGE_ID + ++messageSeq;

function stateOf(bot: MockBot): BotState {
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

function rememberMessage(st: BotState, payload: Record<string, unknown>): void {
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

function flushNow(st: BotState, sub: Subscriber): void {
    if (sub.flushTimer !== null) {
        clearTimeout(sub.flushTimer);
        sub.flushTimer = null;
    }
    if (sub.queue.length === 0) return;
    const events = sub.queue;
    sub.queue = [];
    sub.onBatch({ v: EVENT_VERSION, bot_id: st.bot.id, events });
}

function scheduleFlush(st: BotState, sub: Subscriber): void {
    if (sub.flushTimer !== null) return;
    sub.flushTimer = setTimeout(() => {
        sub.flushTimer = null;
        flushNow(st, sub);
    }, BATCH_FLUSH_MS);
}

function trimRing(st: BotState): void {
    if (st.ring.length < RING_CAP + RING_TRIM_SLACK) return;
    const drop = st.ring.length - RING_CAP;
    st.ring.splice(0, drop);
    st.droppedTotal += drop;
    pushBodyAt(st, { kind: 'dropped', count: drop }, Date.now());
}

/** 写进缓冲并推给所有订阅者；这个 Bot 没有接收器就什么都不记（后端也是不收的） */
function pushBodyAt(st: BotState, body: DebugEventBody, atMs: number): void {
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

const pushBody = (st: BotState, body: DebugEventBody) => pushBodyAt(st, body, Date.now());

function pushOb11At(st: BotState, payload: Record<string, unknown>, atMs: number): void {
    if (payload.post_type === 'message' || payload.post_type === 'message_sent') rememberMessage(st, payload);
    pushBodyAt(st, { kind: 'ob11', payload }, atMs);
}

const pushOb11 = (st: BotState, payload: Record<string, unknown>) => pushOb11At(st, payload, Date.now());

// —— 接收器与定时器

function stopHeartbeat(rc: Receiver): void {
    if (rc.heartbeatTimer !== null) clearTimeout(rc.heartbeatTimer);
    rc.heartbeatTimer = null;
}

function stopTimers(rc: Receiver): void {
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
const sendsHeartbeat = (st: BotState, rc: Receiver): boolean =>
    !(st.bot.backend === 'napcat' && rc.source.kind === 'internal');

function scheduleNextEvent(st: BotState, rc: Receiver): void {
    rc.eventTimer = setTimeout(() => {
        rc.eventTimer = null;
        pushOb11(st, makeRandomEvent(st.ctx, Date.now()));
        scheduleNextEvent(st, rc);
    }, randInt(st.rng, 1200, 3000));
}

function scheduleNextHeartbeat(st: BotState, rc: Receiver): void {
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
function scheduleProbe(st: BotState, rc: Receiver): void {
    rc.probeTimer = setTimeout(() => {
        rc.probeTimer = null;
        for (const sub of [...rc.subs.values()]) sub.onBatch({ v: EVENT_VERSION, bot_id: st.bot.id, events: [] });
        // 回调里可能已经退订到零或停了接收器，这时不能再挂下一轮
        if (st.receiver === rc && rc.subs.size > 0 && rc.probeTimer === null) scheduleProbe(st, rc);
    }, PROBE_EVERY_MS);
}

/** 每次有人订阅都走这里；换过事件来源时心跳要跟着开或关 */
function startTimers(st: BotState, rc: Receiver): void {
    if (rc.eventTimer === null) scheduleNextEvent(st, rc);
    if (!sendsHeartbeat(st, rc)) stopHeartbeat(rc);
    else if (rc.heartbeatTimer === null) scheduleNextHeartbeat(st, rc);
    if (rc.probeTimer === null) scheduleProbe(st, rc);
    rc.pausedAt = null;
}

/** 没人订阅的这段时间里「后端」还在收，补一批带真实时间戳的事件，最多 100 条 */
function catchUp(st: BotState, from: number, to: number): void {
    let at = from;
    for (let n = 0; n < 100; n += 1) {
        at += randInt(st.rng, 1200, 3000);
        if (at >= to) break;
        pushOb11At(st, makeRandomEvent(st.ctx, at), at);
    }
}

function seedReceiver(st: BotState, source: DebugChannelId): void {
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

function ensureReceiver(st: BotState, source: DebugChannelId): Receiver {
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

function receiverInfo(st: BotState): DebugReceiverInfo | null {
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

function shutdownReceiver(st: BotState, reason: string): void {
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
function flood(n: number, botId?: string): number {
    let pushed = 0;
    for (const st of states.values()) {
        if (botId && st.bot.id !== botId) continue;
        if (!st.receiver || st.receiver.subs.size === 0) continue;
        for (let i = 0; i < n; i += 1) pushOb11(st, makeGroupMessage(st.ctx, Date.now()));
        pushed += n;
    }
    return pushed;
}

// ---------------------------------------------------------------------------
// 调用
// ---------------------------------------------------------------------------

interface Reply {
    retcode: number;
    data: unknown;
    message?: string;
    wording?: string;
}

interface HandlerCtx {
    st: BotState;
    params: Record<string, unknown>;
}

type Handler = (c: HandlerCtx) => Reply;

const okReply = (data: unknown = null): Reply => ({ retcode: 0, data });
const failReply = (retcode: number, wording: string): Reply => ({ retcode, data: null, message: wording, wording });

const numParam = (params: Record<string, unknown>, key: string): number => {
    const v = params[key];
    if (typeof v === 'number') return v;
    if (typeof v === 'string' && v.trim() !== '') return Number(v);
    return Number.NaN;
};

function groupRow(groupId: number) {
    return {
        group_id: groupId,
        group_name: groupNameOf(groupId),
        member_count: memberCountOf(groupId),
        max_member_count: groupId === GROUP_ID_BASE + 1 ? BIG_GROUP_MEMBER_COUNT : 200,
        group_all_shut: 0,
        group_remark: '',
    };
}

const memberCache = new Map<number, Array<Record<string, unknown>>>();

function membersOf(groupId: number): Array<Record<string, unknown>> {
    let list = memberCache.get(groupId);
    if (!list) {
        list = Array.from({ length: memberCountOf(groupId) }, (_, i) => {
            const p = personAt(i + 1);
            return {
                group_id: groupId,
                user_id: p.id,
                nickname: p.nickname,
                card: p.card,
                sex: 'unknown',
                age: 0,
                area: '',
                level: String(1 + ((i * 3) % 60)),
                qq_level: 0,
                join_time: 1_650_000_000 + i * 86_400,
                last_sent_time: 1_759_000_000 - i * 600,
                title_expire_time: 0,
                unfriendly: false,
                card_changed: false,
                is_robot: false,
                shut_up_timestamp: 0,
                role: p.role,
                title: '',
            };
        });
        memberCache.set(groupId, list);
    }
    return list;
}

function friendRow(p: MockPerson) {
    return { user_id: p.id, nickname: p.nickname, remark: p.card, sex: 'unknown', age: 0, level: 0 };
}

const knownGroup = (params: Record<string, unknown>): number | null => {
    const id = numParam(params, 'group_id');
    return isKnownGroup(id) ? id : null;
};

function sendMessage(
    c: HandlerCtx,
    messageType: 'group' | 'private',
    targetId: number,
    segments: Ob11Segment[],
): Reply {
    if (segments.length === 0) return failReply(1400, 'message 不能为空');
    const messageId = nextMessageId();
    // 上游会把自己发的消息再报回来（message_sent），和调用回包分别到达，界面要按 message_id 合并
    pushOb11(c.st, makeSelfMessage(c.st.ctx, Date.now(), { messageId, messageType, targetId, segments }));
    return okReply({ message_id: messageId });
}

function pushNotice(c: HandlerCtx, notice: Record<string, unknown>): void {
    pushOb11(c.st, makeActionNotice(c.st.ctx, Date.now(), notice));
}

const HANDLERS: Record<string, Handler> = {
    get_login_info: (c) => okReply({ user_id: c.st.bot.qq, nickname: c.st.bot.name }),
    get_status: (c) => okReply({ online: c.st.bot.online ?? true, good: true, stat: {} }),
    get_version_info: (c) =>
        okReply(
            c.st.bot.backend === 'napcat'
                ? { app_name: 'NapCat.Onebot', app_version: '4.15.18', protocol_version: 'v11' }
                : { app_name: 'SnowLuma', app_version: '0.9.0', protocol_version: 'v11' },
        ),
    bot_exit: () => okReply(),

    send_group_msg: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, `群 ${String(c.params.group_id)} 不存在或没有加入`);
        return sendMessage(c, 'group', groupId, messageToSegments(c.params.message));
    },
    send_private_msg: (c) => {
        const userId = numParam(c.params, 'user_id');
        if (!Number.isFinite(userId)) return failReply(1400, 'user_id 不合法');
        return sendMessage(c, 'private', userId, messageToSegments(c.params.message));
    },
    send_msg: (c) => {
        const type = c.params.message_type ?? (c.params.group_id !== undefined ? 'group' : 'private');
        return type === 'group' ? HANDLERS.send_group_msg(c) : HANDLERS.send_private_msg(c);
    },
    send_group_forward_msg: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, `群 ${String(c.params.group_id)} 不存在或没有加入`);
        const resId = `res_${nextMessageId().toString(16)}`;
        const reply = sendMessage(c, 'group', groupId, [{ type: 'forward', data: { id: resId } }]);
        return reply.retcode === 0 ? okReply({ ...(reply.data as object), res_id: resId }) : reply;
    },
    get_msg: (c) => {
        const found = c.st.messages.get(numParam(c.params, 'message_id'));
        if (!found) return failReply(1200, '消息不存在');
        return okReply({
            time: found.time,
            message_type: found.message_type,
            message_id: found.message_id,
            real_id: found.real_id,
            sender: found.sender,
            message: found.message,
            raw_message: found.raw_message,
            ...(found.group_id !== undefined ? { group_id: found.group_id } : {}),
        });
    },
    delete_msg: (c) => {
        const id = numParam(c.params, 'message_id');
        const found = c.st.messages.get(id);
        if (!found) return failReply(1200, '消息不存在或已超过撤回时限');
        c.st.messages.delete(id);
        pushNotice(
            c,
            found.message_type === 'group'
                ? {
                      notice_type: 'group_recall',
                      group_id: found.group_id,
                      user_id: found.user_id,
                      operator_id: c.st.bot.qq,
                      message_id: id,
                  }
                : { notice_type: 'friend_recall', user_id: found.user_id, message_id: id },
        );
        return okReply();
    },
    get_forward_msg: (c) => {
        const from = numParam(c.params, 'message_id');
        const sample = c.st.messages.get(from);
        const people = [personAt(1), personAt(2)];
        return okReply({
            messages: people.map((p, i) => ({
                sender: { user_id: p.id, nickname: p.nickname },
                time: 1_759_190_400 + i * 60,
                message: sample && i === 0 ? sample.message : [{ type: 'text', data: { text: `转发的第 ${i + 1} 条` } }],
            })),
        });
    },

    get_group_list: () =>
        okReply(Array.from({ length: GROUP_COUNT }, (_, i) => groupRow(GROUP_ID_BASE + i + 1))),
    get_group_info: (c) => {
        const groupId = knownGroup(c.params);
        return groupId === null ? failReply(1200, '群不存在') : okReply(groupRow(groupId));
    },
    get_group_member_list: (c) => {
        const groupId = knownGroup(c.params);
        return groupId === null ? failReply(1200, '群不存在') : okReply(membersOf(groupId));
    },
    get_group_member_info: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        const userId = numParam(c.params, 'user_id');
        const hit = membersOf(groupId).find((m) => m.user_id === userId);
        return hit ? okReply(hit) : failReply(1200, '群成员不存在');
    },
    set_group_ban: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        const duration = numParam(c.params, 'duration');
        const seconds = Number.isFinite(duration) ? duration : 1800;
        pushNotice(c, {
            notice_type: 'group_ban',
            sub_type: seconds > 0 ? 'ban' : 'lift_ban',
            group_id: groupId,
            operator_id: c.st.bot.qq,
            user_id: numParam(c.params, 'user_id'),
            duration: seconds,
        });
        return okReply();
    },
    set_group_kick: (c) => {
        const groupId = knownGroup(c.params);
        if (groupId === null) return failReply(1200, '群不存在');
        pushNotice(c, {
            notice_type: 'group_decrease',
            sub_type: 'kick',
            group_id: groupId,
            operator_id: c.st.bot.qq,
            user_id: numParam(c.params, 'user_id'),
        });
        return okReply();
    },
    set_group_whole_ban: (c) => (knownGroup(c.params) === null ? failReply(1200, '群不存在') : okReply()),
    set_group_card: (c) => (knownGroup(c.params) === null ? failReply(1200, '群不存在') : okReply()),

    get_friend_list: () => okReply(Array.from({ length: FRIEND_COUNT }, (_, i) => friendRow(personAt(i + 1)))),
    get_stranger_info: (c) => {
        const userId = numParam(c.params, 'user_id');
        const n = userId - USER_ID_BASE;
        const person = n >= 1 && n <= 500 ? personAt(n) : { id: userId, nickname: `陌生人 ${userId}` };
        return okReply({
            user_id: person.id,
            nickname: person.nickname,
            sex: 'unknown',
            age: 0,
            qid: '',
            long_nick: '',
            reg_time: 1_500_000_000,
            is_vip: false,
        });
    },
    send_like: (c) => {
        const times = numParam(c.params, 'times');
        return Number.isFinite(times) && times > 20 ? failReply(1200, '今日点赞次数已达上限') : okReply();
    },

    get_group_file_url: () => okReply({ url: 'https://example.invalid/group-file/abcd-1234' }),
    fetch_custom_face: (c) => {
        const count = numParam(c.params, 'count');
        const n = Math.min(Number.isFinite(count) ? count : 48, 12);
        return okReply(Array.from({ length: n }, (_, i) => placeholderImage(i + 1, 96, 96)));
    },
    nc_get_rkey: () =>
        okReply([
            { type: 'private', rkey: '&rkey=CAQSKAB6JWENi5LM', created_at: 1_759_190_400, ttl: 86_400 },
            { type: 'group', rkey: '&rkey=CAESKAB6JWENi5LM', created_at: 1_759_190_400, ttl: 86_400 },
        ]),
    get_group_album_list: (c) =>
        knownGroup(c.params) === null
            ? failReply(1200, '群不存在')
            : okReply([
                  { album_id: 'album_1', name: '聚会', upload_number: 12 },
                  { album_id: 'album_2', name: '截图', upload_number: 87 },
              ]),

    // 预览专用：一份约 5.7 MiB 的回包，走一遍「截断 → 只看预览 → 另存完整内容」。
    // 行是按序号算出来的，同样的调用永远回同样的内容；带中文，预览的截断点才会碰上多字节字符
    debug_huge_response: () =>
        okReply({
            note: '预览用的超大回包：结果区只显示前 256 KiB，完整内容请另存',
            rows: Array.from({ length: HUGE_RESPONSE_ROWS }, (_, i) => {
                const p = personAt((i % BIG_GROUP_MEMBER_COUNT) + 1);
                return {
                    seq: i + 1,
                    group_id: GROUP_ID_BASE + 1 + (i % GROUP_COUNT),
                    user_id: p.id,
                    nickname: p.nickname,
                    text: `第 ${i + 1} 行：预览用的填充数据`,
                };
            }),
        }),
};

/** 目录里有、但没写专门回包的动作（处理请求、上传删除文件等）：照成功回一个空结果 */
const GENERIC_OK: Handler = () => okReply();

/**
 * 扫文本的 UTF-8 字节数，扫到超过 maxBytes 之前停下，返回停在第几个码元、累计多少字节。
 * 后端的体积都按字节算，JS 的 length 是 UTF-16 码元数，对不上；代理对算一个 4 字节字符，
 * 所以停下的位置永远在字符边界上。超大回包要扫近 6 MB，逐字符不分配对象
 */
function scanUtf8(text: string, maxBytes: number): { end: number; bytes: number } {
    let bytes = 0;
    let i = 0;
    while (i < text.length) {
        const code = text.charCodeAt(i);
        const pair = code >= 0xd800 && code <= 0xdbff && i + 1 < text.length;
        const size = code < 0x80 ? 1 : code < 0x800 ? 2 : pair ? 4 : 3;
        if (bytes + size > maxBytes) break;
        bytes += size;
        i += pair ? 2 : 1;
    }
    return { end: i, bytes };
}

const utf8Length = (text: string): number => scanUtf8(text, Number.POSITIVE_INFINITY).bytes;

/** 前 maxBytes 个 UTF-8 字节，不切在字符中间，和后端截预览的做法一致 */
const utf8Prefix = (text: string, maxBytes: number): string => text.slice(0, scanUtf8(text, maxBytes).end);

/** 最近几次被截断的回包全文，「另存完整内容」用；没截断的回包界面手里本来就是全的，不留 */
const largeResponses = new Map<string, string>();
/** 「另存」和「导出收藏」写到的假文件，key 是路径 */
const files = new Map<string, string>();

function keepLargeResponse(requestId: string, text: string): void {
    // 先删再放，同一个请求号重来一次也排到最新
    largeResponses.delete(requestId);
    largeResponses.set(requestId, text);
    while (largeResponses.size > LARGE_RESPONSES_KEPT) {
        const oldest = largeResponses.keys().next();
        if (oldest.done) break;
        largeResponses.delete(oldest.value);
    }
}

/**
 * 回包折算成界面看的结果，截断规则照后端：文本超过 5 MiB 才截，截了 `data` 置空、
 * `raw` 换成前 256 KiB 的文本预览；status / retcode / message / wording 仍取自完整回包，
 * 结果头部照常显示，size_bytes 是完整回包的字节数
 */
function buildOutcome(
    reply: Reply,
    requestId: string,
    backend: BackendType,
    channel: DebugChannelId,
    elapsedMs: number,
): DebugCallOutcome {
    const status = reply.retcode === 0 ? 'ok' : 'failed';
    const message = reply.message ?? '';
    const wording = reply.wording ?? '';
    const raw: Record<string, unknown> = {
        status,
        retcode: reply.retcode,
        data: reply.data,
        message,
        wording,
        echo: requestId,
        ...(backend === 'napcat' ? { stream: 'normal-action' } : {}),
    };
    const text = JSON.stringify(raw);
    const size = utf8Length(text);
    const truncated = size > RESPONSE_INLINE_LIMIT;
    if (truncated) keepLargeResponse(requestId, text);
    return {
        ok: reply.retcode === 0,
        status,
        retcode: reply.retcode,
        data: truncated ? null : reply.data,
        message,
        wording,
        raw: truncated ? utf8Prefix(text, RAW_PREVIEW_LIMIT) : raw,
        elapsed_ms: elapsedMs,
        channel,
        size_bytes: size,
        truncated,
    };
}

interface CallPlan {
    delay: number;
    channel: DebugChannelId | null;
    bot: MockBot | null;
    run: (elapsedMs: number) => DebugCallResult;
}

const failNow = (error: DebugError, bot: MockBot | null = null): CallPlan => ({
    delay: 60,
    channel: null,
    bot,
    run: () => ({ kind: 'err', error }),
});

function planCall(request: DebugCallRequest): CallPlan {
    const bot = findBot(request.bot_id);
    if (!bot) return failNow({ kind: 'bot_not_found' });
    const resolved = resolveChannel(bot, request.channel, 'call');
    if (!resolved.ok) return failNow(resolved.error, bot);
    const channel = resolved.def.id;
    if (request.params !== null && request.params !== undefined && !isRecord(request.params)) {
        return failNow({ kind: 'invalid_params', message: '参数必须是 JSON 对象' }, bot);
    }
    const params = isRecord(request.params) ? request.params : {};
    const spec = buildMockSpec(bot.backend, request.action, { source: 'live' });
    const known = spec !== null && !bot.missingActions.has(spec.name);
    const st = stateOf(bot);
    const delay = spec?.name === 'get_group_member_list' ? MEMBER_LIST_DELAY_MS : randInt(callRng, 60, 400);

    const outcome = (reply: Reply, elapsedMs: number): DebugCallResult => ({
        kind: 'ok',
        outcome: buildOutcome(reply, request.request_id, bot.backend, channel, elapsedMs),
    });

    return {
        delay,
        channel,
        bot,
        run: (elapsedMs) => {
            if (!known) return outcome(failReply(1404, `不支持的 API: ${request.action}`), elapsedMs);
            if (spec.stream) {
                return { kind: 'err', error: { kind: 'invalid_params', message: '流式接口暂不支持调用，先看文档' } };
            }
            const missing = (mockRequiredParams(bot.backend, spec.name) ?? []).filter(
                (name) => params[name] === undefined || params[name] === null || params[name] === '',
            );
            if (missing.length > 0) {
                return outcome(failReply(1400, `缺少必需参数：${missing.join('、')}`), elapsedMs);
            }
            const handler = HANDLERS[spec.name] ?? GENERIC_OK;
            return outcome(handler({ st, params }), elapsedMs);
        },
    };
}

// —— 进行中的调用

const pending = new Map<string, { cancel: () => void }>();

// —— 历史

let historyStore: DebugHistoryEntry[] = [];
let historySeq = 0;
let mockStorageNotices: DebugStorageNotice[] = [];

const isCredentialAction = (action: string): boolean =>
    CREDENTIAL_ACTIONS.has(action.endsWith('_async') ? action.slice(0, -'_async'.length) : action);

/**
 * 历史里存的回包，规则照后端入库：存的是结果里的 `raw`（被截断的调用就是那段预览）；
 * 序列化后超过 256 KiB 就整个不存、标成被截断，调用时已经截过的保持截断；
 * 凭据类动作一律不存，也不算截断
 */
function historyResponse(action: string, outcome: DebugCallOutcome | null): { response: unknown; truncated: boolean } {
    if (!outcome || isCredentialAction(action)) return { response: null, truncated: false };
    const text = JSON.stringify(outcome.raw);
    if (utf8Length(text) > HISTORY_RESPONSE_LIMIT) return { response: null, truncated: true };
    return { response: JSON.parse(text) as unknown, truncated: outcome.truncated };
}

// —— 参数瘦身：照后端 ncd-runtime onebot_debug/params.rs 的同一套规则，
// 历史和事件流里的调用记录都要走它，预览里才能看到「占位文字被拦住」这条链路

const PARAM_STRING_LIMIT = 64 * 1024;
const PARAM_TOTAL_LIMIT = 256 * 1024;
const KEPT_SCALAR_LIMIT = 256;

const placeholder = (bytes: number): string => `<已省略 ${bytes} 字节>`;

function slimLongStrings(value: unknown): { v: unknown; changed: boolean } {
    if (typeof value === 'string') {
        if (utf8Length(value) <= PARAM_STRING_LIMIT) return { v: value, changed: false };
        return { v: placeholder(utf8Length(value)), changed: true };
    }
    if (Array.isArray(value)) {
        let changed = false;
        const next = value.map((item) => {
            const r = slimLongStrings(item);
            changed ||= r.changed;
            return r.v;
        });
        return changed ? { v: next, changed } : { v: value, changed };
    }
    if (isRecord(value)) {
        let changed = false;
        const next: Record<string, unknown> = {};
        for (const [key, field] of Object.entries(value)) {
            const r = slimLongStrings(field);
            changed ||= r.changed;
            next[key] = r.v;
        }
        return changed ? { v: next, changed } : { v: value, changed };
    }
    return { v: value, changed: false };
}

/** 照后端的 cap_record_params：先换掉超长字符串，整体还大就按顶层字段收，仍大收成摘要 */
function slimRecordedParams(value: unknown): { v: unknown; changed: boolean } {
    const slim = slimLongStrings(value);
    let v = slim.v;
    let changed = slim.changed;
    const total = utf8Length(JSON.stringify(v));
    if (total <= PARAM_TOTAL_LIMIT) return { v, changed };
    if (isRecord(v)) {
        const next: Record<string, unknown> = {};
        for (const [key, field] of Object.entries(v)) {
            const short =
                field === null ||
                typeof field === 'boolean' ||
                typeof field === 'number' ||
                (typeof field === 'string' && utf8Length(field) <= KEPT_SCALAR_LIMIT);
            next[key] = short ? field : placeholder(utf8Length(JSON.stringify(field)));
            if (!short) changed = true;
        }
        v = next;
    }
    if (utf8Length(JSON.stringify(v)) > PARAM_TOTAL_LIMIT) {
        v = { _omitted: `<参数共 ${total} 字节，已省略>` };
        changed = true;
    }
    return { v, changed };
}

function addHistory(
    bot: MockBot,
    request: DebugCallRequest,
    channel: DebugChannelId,
    result: DebugCallResult,
    elapsedMs: number,
): void {
    if (request.origin !== 'editor' && request.origin !== 'composer') return;
    const outcome = result.kind === 'ok' ? result.outcome : null;
    const stored = historyResponse(request.action, outcome);
    const slim = slimRecordedParams(clone(request.params ?? {}));
    historyStore.unshift({
        id: `mock-hist-${++historySeq}`,
        at_ms: Date.now(),
        bot_id: bot.id,
        bot_name: bot.name,
        backend: bot.backend,
        channel,
        origin: request.origin,
        action: request.action,
        params: slim.v,
        params_truncated: slim.changed,
        ok: outcome ? outcome.ok : false,
        retcode: outcome ? outcome.retcode : null,
        error: result.kind === 'err' ? result.error : null,
        elapsed_ms: elapsedMs,
        response: stored.response,
        response_truncated: stored.truncated,
    });
    if (historyStore.length > HISTORY_CAP) historyStore = historyStore.slice(0, HISTORY_CAP);
}

/** 我们自己发的调用也记进事件流，界面据此在聊天里合并气泡、画调用小标签 */
function recordCallEvent(
    bot: MockBot,
    request: DebugCallRequest,
    channel: DebugChannelId | null,
    result: DebugCallResult,
    elapsedMs: number,
): void {
    // picker 发起的查询（拉群 / 成员列表）后端不写进事件流，聊天里不冒调用小标签
    if (request.origin === 'picker') return;
    const st = states.get(bot.id);
    if (!st?.receiver) return;
    const outcome = result.kind === 'ok' ? result.outcome : null;
    const data = outcome?.data;
    const messageId = isRecord(data) && typeof data.message_id === 'number' ? data.message_id : null;
    pushBody(st, {
        kind: 'call',
        record: {
            request_id: request.request_id,
            origin: request.origin,
            action: request.action,
            params: slimRecordedParams(request.params ?? {}).v,
            ok: outcome ? outcome.ok : false,
            retcode: outcome ? outcome.retcode : null,
            elapsed_ms: elapsedMs,
            message_id: messageId,
            error: result.kind === 'err' ? errorText(result.error) : null,
            channel,
        },
    });
}

function call(request: DebugCallRequest): Promise<DebugCallResponse> {
    const startedAt = Date.now();
    const plan = planCall(request);
    const timeoutMs = request.timeout_ms && request.timeout_ms > 0 ? request.timeout_ms : DEFAULT_TIMEOUT_MS;

    return new Promise<DebugCallResponse>((resolve) => {
        let workTimer: Timer | null = null;
        let limitTimer: Timer | null = null;
        let done = false;

        const finish = (result: DebugCallResult) => {
            if (done) return;
            done = true;
            if (workTimer !== null) clearTimeout(workTimer);
            if (limitTimer !== null) clearTimeout(limitTimer);
            pending.delete(request.request_id);
            const elapsed = Date.now() - startedAt;
            if (plan.bot) {
                recordCallEvent(plan.bot, request, plan.channel, result, elapsed);
                addHistory(plan.bot, request, plan.channel ?? request.channel, result, elapsed);
            }
            resolve({ request_id: request.request_id, result });
        };

        pending.set(request.request_id, { cancel: () => finish({ kind: 'err', error: { kind: 'cancelled' } }) });
        workTimer = setTimeout(() => {
            workTimer = null;
            finish(plan.run(Date.now() - startedAt));
        }, plan.delay);
        limitTimer = setTimeout(() => {
            limitTimer = null;
            finish({ kind: 'err', error: { kind: 'timeout', ms: timeoutMs } });
        }, timeoutMs);
    });
}

// ---------------------------------------------------------------------------
// 工作区 / 收藏
// ---------------------------------------------------------------------------

const defaultWorkspace = (): DebugWorkspace => ({
    version: 1,
    tabs: [],
    active_tab: null,
    closed_tabs: [],
    selected_bot: null,
    channel_choice: {},
    layout: { left_collapsed: false, right_collapsed: false, left_width: 240, right_width: 380, right_view: 'chat' },
    recent_actions: [],
});

const seedCollections = (): DebugCollections => ({
    version: 1,
    folders: [{ id: 'mock-folder-common', name: '常用', order: 0 }],
    requests: [
        {
            id: 'mock-req-login',
            name: '看看登录号',
            folder_id: 'mock-folder-common',
            action: 'get_login_info',
            params: {},
            channel: null,
            note: null,
            order: 0,
            created_at_ms: SEED_EPOCH_MS,
            updated_at_ms: SEED_EPOCH_MS,
        },
        {
            id: 'mock-req-hello',
            name: '测试群打招呼',
            folder_id: 'mock-folder-common',
            action: 'send_group_msg',
            params: { group_id: '100001', message: '大家好，这是一条调试消息' },
            channel: null,
            note: '发到测试群 1',
            order: 1,
            created_at_ms: SEED_EPOCH_MS + 60_000,
            updated_at_ms: SEED_EPOCH_MS + 60_000,
        },
        {
            id: 'mock-req-groups',
            name: '群列表（走内部通道）',
            folder_id: null,
            action: 'get_group_list',
            params: {},
            channel: { kind: 'internal' },
            note: null,
            order: 0,
            created_at_ms: SEED_EPOCH_MS + 120_000,
            updated_at_ms: SEED_EPOCH_MS + 120_000,
        },
    ],
});

let workspaceStore: DebugWorkspace = defaultWorkspace();
let collectionsStore: DebugCollections = seedCollections();

function mergeCollections(incoming: DebugCollections): void {
    const folderIds = new Set(collectionsStore.folders.map((f) => f.id));
    const requestIds = new Set(collectionsStore.requests.map((r) => r.id));
    const folderMap = new Map<string, string>();
    const folderBase = collectionsStore.folders.reduce((m, f) => Math.max(m, f.order + 1), 0);
    incoming.folders.forEach((f, i) => {
        let id = f.id;
        for (let n = 2; folderIds.has(id); n += 1) id = `${f.id}-${n}`;
        folderIds.add(id);
        folderMap.set(f.id, id);
        collectionsStore.folders.push({ ...f, id, order: folderBase + i });
    });
    incoming.requests.forEach((r) => {
        let id = r.id;
        for (let n = 2; requestIds.has(id); n += 1) id = `${r.id}-${n}`;
        requestIds.add(id);
        collectionsStore.requests.push({
            ...r,
            id,
            folder_id: r.folder_id === null ? null : (folderMap.get(r.folder_id) ?? null),
        });
    });
}

// ---------------------------------------------------------------------------
// 对外接口
// ---------------------------------------------------------------------------

function catalogFlavor(botId: string | null): MockSpecOptions {
    const bot = botId ? findBot(botId) : undefined;
    // 没选 Bot 或 Bot 没在跑：只有内置快照；在线目录里缺的动作由这个 Bot 的老版本决定
    if (!bot || !bot.running) return { source: 'snapshot' };
    return { source: 'live', unsupported: bot.missingActions };
}

export const onebotDebugMock = {
    targets: (): Promise<DebugTarget[]> =>
        respond(
            BOTS.map((b) => ({
                bot_id: b.id,
                name: b.name,
                qq_id: b.qq,
                backend: b.backend,
                host: clone(b.host),
                running: b.running,
                online: b.online,
            })),
        ),

    channels: (botId: string): Promise<DebugChannels> => {
        const bot = findBot(botId);
        if (!bot) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const infos = channelDefs(bot).map((d) => channelInfo(bot, d));
        return respond({
            bot_id: bot.id,
            channels: infos,
            auto_call: pickAuto(bot, 'call'),
            auto_events: pickAuto(bot, 'events'),
        });
    },

    testChannel: (botId: string, channel: DebugChannelId): Promise<DebugChannelInfo> => {
        const bot = findBot(botId);
        if (!bot) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const target = channel.kind === 'auto' ? pickAuto(bot, 'call') : channel;
        const def = target ? channelDefs(bot).find((d) => channelKey(d.id) === channelKey(target)) : undefined;
        if (!def) return rejectAfterDelay('这个 Bot 没有这条通道');
        const generation = mockGeneration;
        // 和后端一样测完才记结果：探测进行中拉通道列表，看到的还是旧状态
        return new Promise((resolve) => {
            setTimeout(() => {
                // 测的途中预览被重置过，结果就作废，不往新的状态里写
                if (generation === mockGeneration) {
                    const before = currentStatus(bot, def);
                    // 没测过的 WS 通道测一下就通了；令牌错的 HTTP 服务被上游拒绝
                    let after = before;
                    if (before.kind === 'unknown') after = { kind: 'available' };
                    else if (before.kind === 'tunneled' && def.id.kind === 'http' && bot.httpRejectsToken) {
                        after = { kind: 'auth_failed', status: 401 };
                    }
                    if (after !== before) statusOverrides.set(overrideKey(bot.id, def.id), after);
                }
                resolve(channelInfo(bot, def));
            }, TEST_CHANNEL_MS);
        });
    },

    catalog: (botId: string | null, backend: BackendType): Promise<DebugCatalog> =>
        respond(buildMockCatalog(backend, catalogFlavor(botId))),

    describe: (botId: string | null, backend: BackendType, action: string): Promise<DebugActionSpec | null> =>
        respond(buildMockSpec(backend, action, catalogFlavor(botId))),

    call,

    cancel: (requestId: string): Promise<void> => {
        pending.get(requestId)?.cancel();
        return Promise.resolve();
    },

    /** 只认最近几次被截断的调用；没截断的回包界面里已经是全的，后端也不留全文 */
    saveResponse: (requestId: string, path: string): Promise<void> => {
        const text = largeResponses.get(requestId);
        if (text === undefined) {
            return rejectAfterDelay(`没有这次调用的完整回包：只保留最近 ${LARGE_RESPONSES_KEPT} 次被截断的回包`);
        }
        files.set(path, text);
        return respond(undefined);
    },

    subscribe: async (
        botId: string,
        source: DebugChannelId,
        onBatch: (batch: DebugEventBatch) => void,
    ): Promise<DebugSubscribeResponse> => {
        await respond(undefined);
        const bot = findBot(botId);
        if (!bot) throw errorText({ kind: 'bot_not_found' });
        const resolved = resolveChannel(bot, source, 'events');
        if (!resolved.ok) throw errorText(resolved.error);

        const st = stateOf(bot);
        const rc = ensureReceiver(st, resolved.def.id);
        if (rc.subs.size === 0 && rc.pausedAt !== null) catchUp(st, rc.pausedAt, Date.now());

        // 先把缓冲里已有的按 500 条一批补给它，再登记进实时推送：中间不漏也不重
        for (let i = 0; i < st.ring.length; i += BACKLOG_CHUNK) {
            onBatch({ v: EVENT_VERSION, bot_id: bot.id, events: st.ring.slice(i, i + BACKLOG_CHUNK) });
        }
        const sub: Subscriber = { id: `mock-sub-${++subscriptionSeq}`, onBatch, queue: [], flushTimer: null };
        rc.subs.set(sub.id, sub);
        subscriptions.set(sub.id, bot.id);
        startTimers(st, rc);

        const receiver = receiverInfo(st);
        if (!receiver) throw '接收器没有建起来';
        return { subscription_id: sub.id, receiver };
    },

    /** 退订立刻生效，不排延迟：页面卸载时不该在浏览器里留定时器 */
    unsubscribe: (subscriptionId: string): Promise<void> => {
        const botId = subscriptions.get(subscriptionId);
        subscriptions.delete(subscriptionId);
        const st = botId ? states.get(botId) : undefined;
        const rc = st?.receiver;
        const sub = rc?.subs.get(subscriptionId);
        if (rc && sub) {
            if (sub.flushTimer !== null) clearTimeout(sub.flushTimer);
            rc.subs.delete(subscriptionId);
            if (rc.subs.size === 0) stopTimers(rc);
        }
        return Promise.resolve();
    },

    receivers: (): Promise<DebugReceiverInfo[]> => {
        const list: DebugReceiverInfo[] = [];
        for (const st of states.values()) {
            const info = receiverInfo(st);
            if (info) list.push(info);
        }
        return respond(list);
    },

    stopReceiver: (botId: string): Promise<void> => {
        const st = states.get(botId);
        if (st) shutdownReceiver(st, '已手动停止');
        return respond(undefined);
    },

    readEvents: (botId: string, sinceSeq: number, limit: number): Promise<DebugEvent[]> => {
        if (!findBot(botId)) return rejectAfterDelay(errorText({ kind: 'bot_not_found' }));
        const ring = states.get(botId)?.ring ?? [];
        return respond(clone(ring.filter((e) => e.seq > sinceSeq).slice(0, Math.max(0, limit))));
    },

    workspace: (): Promise<DebugWorkspace> => respond(clone(workspaceStore)),
    saveWorkspace: (workspace: DebugWorkspace): Promise<void> => {
        workspaceStore = clone(workspace);
        return respond(undefined);
    },

    collections: (): Promise<DebugCollections> => respond(clone(collectionsStore)),
    saveCollections: (collections: DebugCollections): Promise<void> => {
        collectionsStore = clone(collections);
        return respond(undefined);
    },
    exportCollections: (path: string): Promise<void> => {
        files.set(path, JSON.stringify(collectionsStore, null, 2));
        return respond(undefined);
    },
    /** 导入是并进现有收藏：id 撞了的改名，文件夹引用跟着改 */
    importCollections: (path: string): Promise<DebugCollections> => {
        const text = files.get(path);
        if (text === undefined) return rejectAfterDelay(`读不到文件：${path}`);
        let parsed: unknown;
        try {
            parsed = JSON.parse(text);
        } catch {
            return rejectAfterDelay('不是有效的收藏文件');
        }
        if (!isRecord(parsed) || !Array.isArray(parsed.folders) || !Array.isArray(parsed.requests)) {
            return rejectAfterDelay('不是有效的收藏文件');
        }
        mergeCollections(parsed as unknown as DebugCollections);
        return respond(clone(collectionsStore));
    },

    history: (query: DebugHistoryQuery): Promise<DebugHistoryPage> => {
        const text = query.text?.trim().toLowerCase();
        const matched = historyStore.filter(
            (e) =>
                (!query.action || e.action === query.action) &&
                (!query.bot_id || e.bot_id === query.bot_id) &&
                (query.ok === null || query.ok === undefined || e.ok === query.ok) &&
                (!text ||
                    e.action.toLowerCase().includes(text) ||
                    e.bot_name.toLowerCase().includes(text) ||
                    JSON.stringify(e.params).toLowerCase().includes(text)),
        );
        const page = matched.slice(query.offset, query.offset + query.limit);
        return respond({
            entries: page.map((e) => ({
                id: e.id,
                at_ms: e.at_ms,
                bot_id: e.bot_id,
                bot_name: e.bot_name,
                backend: e.backend,
                channel: e.channel,
                origin: e.origin,
                action: e.action,
                ok: e.ok,
                retcode: e.retcode,
                elapsed_ms: e.elapsed_ms,
                error_kind: e.error ? e.error.kind : null,
            })),
            total: matched.length,
        });
    },
    historyEntry: (id: string): Promise<DebugHistoryEntry | null> => {
        const hit = historyStore.find((e) => e.id === id);
        return respond(hit ? clone(hit) : null);
    },
    clearHistory: (): Promise<void> => {
        historyStore = [];
        return respond(undefined);
    },

    // 和后端一样「取走」语义：读过就清空，下一份损坏挪走再出现。预览里想看到横幅，
    // 控制台执行 `__ncdDebugStorageNotice()` 造一条
    storageNotices: (): Promise<DebugStorageNotice[]> => {
        const out = clone(mockStorageNotices);
        mockStorageNotices = [];
        return respond(out);
    },
};

/** 测试用：把所有状态、定时器、计数器恢复到刚加载的样子 */
export function resetOnebotDebugMock(): void {
    for (const p of [...pending.values()]) p.cancel();
    pending.clear();
    for (const st of states.values()) {
        const rc = st.receiver;
        if (!rc) continue;
        for (const sub of rc.subs.values()) if (sub.flushTimer !== null) clearTimeout(sub.flushTimer);
        rc.subs.clear();
        stopTimers(rc);
    }
    states.clear();
    subscriptions.clear();
    statusOverrides.clear();
    memberCache.clear();
    largeResponses.clear();
    files.clear();
    mockGeneration += 1;
    historyStore = [];
    historySeq = 0;
    mockStorageNotices = [];
    messageSeq = 0;
    subscriptionSeq = 0;
    delayRng = mulberry32(0xd31a);
    callRng = mulberry32(0xca11);
    workspaceStore = defaultWorkspace();
    collectionsStore = seedCollections();
}

// 浏览器预览里的压测口子：控制台执行 `__ncdDebugFlood(5000)`，往正在被看的 Bot 一口气灌 5000 条群消息。
// 只在没有 Tauri 的浏览器预览里挂（含 vite preview 出来的生产包，性能工具就是用它），真程序里不存在
if (typeof window !== 'undefined' && !isTauri) {
    (window as unknown as { __ncdDebugFlood?: typeof flood }).__ncdDebugFlood = flood;
    (window as unknown as { __ncdDebugStorageNotice?: () => void }).__ncdDebugStorageNotice = () => {
        mockStorageNotices.push({
            file: 'history.jsonl',
            moved_to: 'history.jsonl.broken-2026-09-30',
            reason: '第 3,812 行不是合法的 JSON',
        });
    };
}
