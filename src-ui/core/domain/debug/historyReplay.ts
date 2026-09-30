// 历史记录「重放」到中栏：把一条完整记录还原成标签页要的参数文本和一次调用结果，
// 这样点开历史时响应面板直接显示当时的回包，不必重新发。

import { debugErrorCopy } from './errorCopy';
import { formatParams } from './paramsText';
import type { DebugCallOutcome } from '../../ipc/generated/debug/DebugCallOutcome';
import type { DebugCallResponse } from '../../ipc/generated/debug/DebugCallResponse';
import type { DebugError } from '../../ipc/generated/debug/DebugError';
import type { DebugHistoryEntry } from '../../ipc/generated/debug/DebugHistoryEntry';

/** 重放出来的结果 request_id 带这个前缀，中栏据此可以认出「这是历史里的，不是刚发的」 */
export const HISTORY_REQUEST_PREFIX = 'history:';

function isPlainObject(v: unknown): v is Record<string, unknown> {
    return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** 参数写回编辑器的文本：对象按编辑器的格式缩进；空的是 `{}`；万一存的不是对象也原样给出，由编辑器报错 */
export function paramsTextOf(params: unknown): string {
    if (params === null || params === undefined) return '{}';
    if (isPlainObject(params)) return formatParams(params);
    return JSON.stringify(params, null, 2) ?? '{}';
}

function byteLength(v: unknown): number {
    if (v === null || v === undefined) return 0;
    const text = typeof v === 'string' ? v : (JSON.stringify(v) ?? '');
    return new TextEncoder().encode(text).length;
}

const text = (v: unknown): string => (typeof v === 'string' ? v : '');

/**
 * 没拿到回包的记录（超时、连不上……）还原成 err；拿到了的从存下来的原始回包里取 status / data 等字段。
 * 回包太大被截断时存的是前一段文本，这时 data 取不出来，只留原文。
 */
export function replayResponse(entry: DebugHistoryEntry): DebugCallResponse {
    const request_id = `${HISTORY_REQUEST_PREFIX}${entry.id}`;
    if (entry.error) return { request_id, result: { kind: 'err', error: entry.error } };

    const raw = entry.response;
    const reply = isPlainObject(raw) ? raw : null;
    const outcome: DebugCallOutcome = {
        ok: entry.ok,
        status: text(reply?.status) || (entry.ok ? 'ok' : 'failed'),
        retcode: entry.retcode ?? (typeof reply?.retcode === 'number' ? reply.retcode : 0),
        data: reply && 'data' in reply ? reply.data : null,
        message: text(reply?.message),
        wording: text(reply?.wording),
        raw,
        elapsed_ms: entry.elapsed_ms,
        channel: entry.channel,
        size_bytes: byteLength(raw),
        truncated: entry.response_truncated,
    };
    return { request_id, result: { kind: 'ok', outcome } };
}

/**
 * 历史列表只带错误种类（`error_kind`），没有载荷；标题正好不依赖载荷，
 * 所以补上空载荷借用同一份错误文案。认不出的种类返回 null。
 */
export function errorKindTitle(kind: string | null): string | null {
    if (!kind) return null;
    try {
        const filler = { kind, reason: '', status: 0, ms: 0, message: '' } as unknown as DebugError;
        return debugErrorCopy(filler).title;
    } catch {
        return null;
    }
}
