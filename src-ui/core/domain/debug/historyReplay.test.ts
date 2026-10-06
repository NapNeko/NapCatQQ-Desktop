import { describe, expect, it } from 'vitest';
import type { DebugHistoryEntry } from '../../ipc/generated/debug/DebugHistoryEntry';
import {
    HISTORY_REQUEST_PREFIX,
    errorKindTitle,
    paramsTextOf,
    replayResponse,
} from './historyReplay';

function entry(patch: Partial<DebugHistoryEntry> = {}): DebugHistoryEntry {
    return {
        id: 'h1',
        at_ms: 1_700_000_000_000,
        bot_id: 'bot-sl',
        bot_name: '小雪',
        backend: 'snowluma',
        channel: { kind: 'internal' },
        origin: 'editor',
        action: 'get_login_info',
        params: { no_cache: true },
        ok: true,
        retcode: 0,
        error: null,
        elapsed_ms: 42,
        response: {
            status: 'ok',
            retcode: 0,
            data: { user_id: 1, nickname: '小雪' },
            message: '',
            wording: '',
        },
        response_truncated: false,
        ...patch,
    };
}

describe('replayResponse', () => {
    it('拿到回包的记录还原成 ok，字段从原始回包里取', () => {
        const r = replayResponse(entry());
        expect(r.request_id).toBe(`${HISTORY_REQUEST_PREFIX}h1`);
        expect(r.result.kind).toBe('ok');
        if (r.result.kind !== 'ok') return;
        expect(r.result.outcome).toMatchObject({
            ok: true,
            status: 'ok',
            retcode: 0,
            data: { user_id: 1, nickname: '小雪' },
            elapsed_ms: 42,
            channel: { kind: 'internal' },
            truncated: false,
        });
        expect(r.result.outcome.size_bytes).toBeGreaterThan(20);
    });

    it('上游报失败（retcode 非 0）照样是 ok 形状，带着 wording', () => {
        const r = replayResponse(
            entry({
                ok: false,
                retcode: 1400,
                response: {
                    status: 'failed',
                    retcode: 1400,
                    data: null,
                    message: 'bad',
                    wording: '参数错误',
                },
            }),
        );
        expect(r.result.kind === 'ok' && r.result.outcome).toMatchObject({
            ok: false,
            status: 'failed',
            retcode: 1400,
            wording: '参数错误',
        });
    });

    it('没拿到回包的记录还原成 err', () => {
        const r = replayResponse(
            entry({
                ok: false,
                retcode: null,
                error: { kind: 'timeout', ms: 60000 },
                response: null,
            }),
        );
        expect(r.result).toEqual({ kind: 'err', error: { kind: 'timeout', ms: 60000 } });
    });

    it('截断的回包只有文本：data 取不出来，原文照留', () => {
        const r = replayResponse(
            entry({ response: '{"status":"ok","data":[1,2,', response_truncated: true }),
        );
        expect(r.result.kind === 'ok' && r.result.outcome).toMatchObject({
            data: null,
            raw: '{"status":"ok","data":[1,2,',
            truncated: true,
        });
    });
});

describe('paramsTextOf / errorKindTitle', () => {
    it('参数写回编辑器的文本', () => {
        expect(paramsTextOf({ a: 1 })).toBe('{\n  "a": 1\n}');
        expect(paramsTextOf(null)).toBe('{}');
        expect(paramsTextOf([1])).toBe('[\n  1\n]');
    });

    it('错误种类借用错误文案的标题，认不出返回 null', () => {
        expect(errorKindTitle('timeout')).toBe('等太久了，调用超时');
        expect(errorKindTitle('transport')).toBe('连接出错');
        expect(errorKindTitle('whatever')).toBeNull();
        expect(errorKindTitle(null)).toBeNull();
    });
});
