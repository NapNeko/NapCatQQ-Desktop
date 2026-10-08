// Bot 清单与通道：有哪些 Bot、每个 Bot 的通道定义与状态折算、自动选路；
// 模拟延迟的 respond / rejectAfterDelay 也放这，所有对外方法共用同一套延迟节奏。

import type { BackendType } from '../../generated/domain/BackendType';
import type { DebugChannelId } from '../../generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../generated/debug/DebugChannelInfo';
import type { DebugChannelStatus } from '../../generated/debug/DebugChannelStatus';
import type { DebugError } from '../../generated/debug/DebugError';
import type { DebugHost } from '../../generated/debug/DebugHost';
import { withMockDelay } from '../bootstrap.mock';
import { mockBots as pageBots } from '../bot.mock';
import { mulberry32, randInt, type Rng } from '../onebot-debug-events.mock';

// ---------------------------------------------------------------------------
// Bot 与通道
// ---------------------------------------------------------------------------

export interface MockBot {
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

export const BOTS: MockBot[] = [
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

export const findBot = (botId: string): MockBot | undefined => BOTS.find((b) => b.id === botId);

export interface ChannelDef {
    id: DebugChannelId;
    label: string;
    canCall: boolean;
    canReceive: boolean;
    port: number | null;
    unsupported?: string;
}

export function channelDefs(bot: MockBot): ChannelDef[] {
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

export const channelKey = (id: DebugChannelId): string =>
    id.kind === 'http' || id.kind === 'ws' ? `${id.kind}:${id.name}` : id.kind;

/** 测试连通 / 调用失败后记下的状态，覆盖初始状态 */
export const statusOverrides = new Map<string, DebugChannelStatus>();
export const overrideKey = (botId: string, id: DebugChannelId) => `${botId}|${channelKey(id)}`;
/** 每重置一次加一：重置之前发起、之后才测完的连通测试据此作废，不把旧结果写进新的状态 */
export let mockGeneration = 0;

export function initialStatus(bot: MockBot, def: ChannelDef): DebugChannelStatus {
    if (def.unsupported) return { kind: 'unsupported', reason: def.unsupported };
    if (!bot.running) return { kind: 'bot_not_running' };
    if (def.id.kind === 'ws') return { kind: 'unknown' };
    if (def.id.kind === 'http' && bot.host.kind !== 'local')
        return { kind: 'tunneled', local_port: bot.tunnelPort };
    return { kind: 'available' };
}

export const currentStatus = (bot: MockBot, def: ChannelDef): DebugChannelStatus =>
    statusOverrides.get(overrideKey(bot.id, def.id)) ?? initialStatus(bot, def);

export function endpointOf(bot: MockBot, def: ChannelDef): string | null {
    if (def.unsupported || def.port === null) return null;
    const tail = def.id.kind === 'internal' ? '/api' : '/';
    return bot.host.kind === 'local'
        ? `127.0.0.1:${def.port}${tail}`
        : `隧道 → 远端 127.0.0.1:${def.port}`;
}

export function channelInfo(bot: MockBot, def: ChannelDef): DebugChannelInfo {
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

export const usable = (s: DebugChannelStatus) =>
    s.kind === 'available' || s.kind === 'tunneled' || s.kind === 'unknown';

/** 「自动」落到哪条：先内部，再 WS，最后 HTTP；实测可用的排在没测过的前面 */
export function pickAuto(bot: MockBot, purpose: 'call' | 'events'): DebugChannelId | null {
    const order = purpose === 'call' ? ['internal', 'ws', 'http'] : ['internal', 'ws'];
    const infos = channelDefs(bot)
        .map((d) => ({ d, status: currentStatus(bot, d) }))
        .filter(
            ({ d, status }) => usable(status) && (purpose === 'call' ? d.canCall : d.canReceive),
        );
    for (const wantKnown of [true, false]) {
        for (const kind of order) {
            const hit = infos.find(
                ({ d, status }) => d.id.kind === kind && (status.kind !== 'unknown') === wantKnown,
            );
            if (hit) return hit.d.id;
        }
    }
    return null;
}

export function statusError(s: DebugChannelStatus): DebugError | null {
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

export type Resolved = { ok: true; def: ChannelDef } | { ok: false; error: DebugError };

export function resolveChannel(
    bot: MockBot,
    id: DebugChannelId,
    purpose: 'call' | 'events',
): Resolved {
    if (!bot.running) return { ok: false, error: { kind: 'bot_not_running' } };
    const target = id.kind === 'auto' ? pickAuto(bot, purpose) : id;
    if (!target)
        return { ok: false, error: { kind: 'channel_unavailable', reason: '没有可用的通道' } };
    const def = channelDefs(bot).find((d) => channelKey(d.id) === channelKey(target));
    if (!def)
        return {
            ok: false,
            error: { kind: 'channel_unavailable', reason: '这个 Bot 没有这条通道' },
        };
    if (purpose === 'events' && !def.canReceive) {
        return {
            ok: false,
            error: { kind: 'channel_unavailable', reason: '这条通道不能接收事件' },
        };
    }
    if (purpose === 'call' && !def.canCall) {
        return {
            ok: false,
            error: { kind: 'channel_unavailable', reason: '这条通道不能发起调用' },
        };
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

export function errorText(error: DebugError): string {
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

export let delayRng: Rng = mulberry32(0xd31a);
export let callRng: Rng = mulberry32(0xca11);

/** 普通读写的模拟延迟：80–300 ms */
export const respond = <T>(value: T): Promise<T> =>
    withMockDelay(value, randInt(delayRng, 80, 300));
export const rejectAfterDelay = (reason: string): Promise<never> =>
    new Promise((_, reject) => setTimeout(() => reject(reason), randInt(delayRng, 80, 300)));

/** 重置通道状态覆盖、代号与延迟随机源；测试和预览重开用 */
export function resetBotsMock(): void {
    statusOverrides.clear();
    mockGeneration += 1;
    delayRng = mulberry32(0xd31a);
    callRng = mulberry32(0xca11);
}
