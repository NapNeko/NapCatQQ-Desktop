// 调用管线：通道解析 → 跑处理器 → 回包折算（超大回包截断成预览）→ 记历史、写事件流的调用记录。

import type { BackendType } from '../../generated/domain/BackendType';
import type { DebugCallOutcome } from '../../generated/debug/DebugCallOutcome';
import type { DebugCallRequest } from '../../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../../generated/debug/DebugCallResponse';
import type { DebugCallResult } from '../../generated/debug/DebugCallResult';
import type { DebugChannelId } from '../../generated/debug/DebugChannelId';
import type { DebugError } from '../../generated/debug/DebugError';
import type { DebugHistoryEntry } from '../../generated/debug/DebugHistoryEntry';
import type { DebugStorageNotice } from '../../generated/debug/DebugStorageNotice';
import { randInt } from '../onebot-debug-events.mock';
import { buildMockSpec, mockRequiredParams } from './catalog-build';
import { findBot, resolveChannel, errorText, respond, callRng, type MockBot } from './bots';
import { states, stateOf, pushBody } from './state';
import { HANDLERS, GENERIC_OK, failReply, type Reply } from './handlers';
import {
    DEFAULT_TIMEOUT_MS,
    MEMBER_LIST_DELAY_MS,
    RESPONSE_INLINE_LIMIT,
    RAW_PREVIEW_LIMIT,
    HISTORY_RESPONSE_LIMIT,
    LARGE_RESPONSES_KEPT,
    HISTORY_CAP,
    CREDENTIAL_ACTIONS,
    clone,
    isRecord,
    type Timer,
} from './consts';

/**
 * 扫文本的 UTF-8 字节数，扫到超过 maxBytes 之前停下，返回停在第几个码元、累计多少字节。
 * 后端的体积都按字节算，JS 的 length 是 UTF-16 码元数，对不上；代理对算一个 4 字节字符，
 * 所以停下的位置永远在字符边界上。超大回包要扫近 6 MB，逐字符不分配对象
 */
export function scanUtf8(text: string, maxBytes: number): { end: number; bytes: number } {
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

export const utf8Length = (text: string): number => scanUtf8(text, Number.POSITIVE_INFINITY).bytes;

/** 前 maxBytes 个 UTF-8 字节，不切在字符中间，和后端截预览的做法一致 */
export const utf8Prefix = (text: string, maxBytes: number): string =>
    text.slice(0, scanUtf8(text, maxBytes).end);

/** 最近几次被截断的回包全文，「另存完整内容」用；没截断的回包界面手里本来就是全的，不留 */
export const largeResponses = new Map<string, string>();
/** 「另存」和「导出收藏」写到的假文件，key 是路径 */
export const files = new Map<string, string>();

export function keepLargeResponse(requestId: string, text: string): void {
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
export function buildOutcome(
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

export interface CallPlan {
    delay: number;
    channel: DebugChannelId | null;
    bot: MockBot | null;
    run: (elapsedMs: number) => DebugCallResult;
}

export const failNow = (error: DebugError, bot: MockBot | null = null): CallPlan => ({
    delay: 60,
    channel: null,
    bot,
    run: () => ({ kind: 'err', error }),
});

export function planCall(request: DebugCallRequest): CallPlan {
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
    const delay =
        spec?.name === 'get_group_member_list' ? MEMBER_LIST_DELAY_MS : randInt(callRng, 60, 400);

    const outcome = (reply: Reply, elapsedMs: number): DebugCallResult => ({
        kind: 'ok',
        outcome: buildOutcome(reply, request.request_id, bot.backend, channel, elapsedMs),
    });

    return {
        delay,
        channel,
        bot,
        run: (elapsedMs) => {
            if (!known)
                return outcome(failReply(1404, `不支持的 API: ${request.action}`), elapsedMs);
            if (spec.stream) {
                return {
                    kind: 'err',
                    error: { kind: 'invalid_params', message: '流式接口暂不支持调用，先看文档' },
                };
            }
            const missing = (mockRequiredParams(bot.backend, spec.name) ?? []).filter(
                (name) =>
                    params[name] === undefined || params[name] === null || params[name] === '',
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

export const pending = new Map<string, { cancel: () => void }>();

// —— 历史

export let historyStore: DebugHistoryEntry[] = [];
export let historySeq = 0;
export let mockStorageNotices: DebugStorageNotice[] = [];

export const isCredentialAction = (action: string): boolean =>
    CREDENTIAL_ACTIONS.has(action.endsWith('_async') ? action.slice(0, -'_async'.length) : action);

/**
 * 历史里存的回包，规则照后端入库：存的是结果里的 `raw`（被截断的调用就是那段预览）；
 * 序列化后超过 256 KiB 就整个不存、标成被截断，调用时已经截过的保持截断；
 * 凭据类动作一律不存，也不算截断
 */
export function historyResponse(
    action: string,
    outcome: DebugCallOutcome | null,
): { response: unknown; truncated: boolean } {
    if (!outcome || isCredentialAction(action)) return { response: null, truncated: false };
    const text = JSON.stringify(outcome.raw);
    if (utf8Length(text) > HISTORY_RESPONSE_LIMIT) return { response: null, truncated: true };
    return { response: JSON.parse(text) as unknown, truncated: outcome.truncated };
}

// —— 参数瘦身：照后端 ncd-runtime onebot_debug/params.rs 的同一套规则，
// 历史和事件流里的调用记录都要走它，预览里才能看到「占位文字被拦住」这条链路

export const PARAM_STRING_LIMIT = 64 * 1024;
export const PARAM_TOTAL_LIMIT = 256 * 1024;
export const KEPT_SCALAR_LIMIT = 256;

export const placeholder = (bytes: number): string => `<已省略 ${bytes} 字节>`;

export function slimLongStrings(value: unknown): { v: unknown; changed: boolean } {
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
export function slimRecordedParams(value: unknown): { v: unknown; changed: boolean } {
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

export function addHistory(
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
export function recordCallEvent(
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
    const messageId =
        isRecord(data) && typeof data.message_id === 'number' ? data.message_id : null;
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

export function call(request: DebugCallRequest): Promise<DebugCallResponse> {
    const startedAt = Date.now();
    const plan = planCall(request);
    const timeoutMs =
        request.timeout_ms && request.timeout_ms > 0 ? request.timeout_ms : DEFAULT_TIMEOUT_MS;

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

        pending.set(request.request_id, {
            cancel: () => finish({ kind: 'err', error: { kind: 'cancelled' } }),
        });
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

/** 测试用：取消进行中的调用，清掉回包 / 历史 / 存储告警缓存 */
export function resetCallMock(): void {
    for (const p of [...pending.values()]) p.cancel();
    pending.clear();
    largeResponses.clear();
    files.clear();
    historyStore = [];
    historySeq = 0;
    mockStorageNotices = [];
}

export function clearHistoryStore(): Promise<void> {
    historyStore = [];
    return respond(undefined);
}

export function takeStorageNotices(): Promise<DebugStorageNotice[]> {
    const out = clone(mockStorageNotices);
    mockStorageNotices = [];
    return respond(out);
}

export function pushStorageNotice(notice: DebugStorageNotice): void {
    mockStorageNotices.push(notice);
}
