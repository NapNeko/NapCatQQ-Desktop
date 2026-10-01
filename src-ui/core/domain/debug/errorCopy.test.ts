import { describe, expect, it } from 'vitest';
import type { DebugError } from '../../ipc/generated/debug/DebugError';
import { NO_CHANNEL_EXITS, UPGRADE_RUNTIME_EXIT } from './channelCopy';
import { callProblem, debugErrorCopy, retcodeHint } from './errorCopy';

describe('debugErrorCopy', () => {
    // 每种 kind 列一遍；源码里的文案表用 satisfies 卡住了 kind 的完整性，这里卡具体文案
    const CASES = [
        { e: { kind: 'bot_not_found' }, title: '找不到这个 Bot', channelIssue: false },
        { e: { kind: 'bot_not_running' }, title: 'Bot 没有在运行', channelIssue: true },
        { e: { kind: 'not_logged_in' }, title: 'QQ 还没登录', channelIssue: true },
        { e: { kind: 'channel_unavailable', reason: '端口没映射' }, title: '这条通道现在用不了', detail: '端口没映射', channelIssue: true },
        { e: { kind: 'upstream_too_old' }, title: '上游版本太老，没有调试接口', detail: '升级到最新版 NapCat / SnowLuma 后可用', channelIssue: true },
        { e: { kind: 'auth_failed', status: 401 }, title: '鉴权失败', detail: '上游返回 401，检查这条通道的 token 是否正确', channelIssue: true },
        { e: { kind: 'timeout', ms: 60000 }, title: '等太久了，调用超时', detail: '已等待 60 秒。上游可能还在执行，可以调大超时后重试', channelIssue: false },
        { e: { kind: 'cancelled' }, title: '已取消', detail: '取消只是不再等待回包，上游可能已经执行了', channelIssue: false },
        { e: { kind: 'transport', message: 'connection refused' }, title: '连接出错', detail: 'connection refused', channelIssue: true },
        { e: { kind: 'invalid_params', message: 'group_id 必填' }, title: '参数有问题', detail: 'group_id 必填', channelIssue: false },
        { e: { kind: 'feature_disabled' }, title: '调试台已关闭', channelIssue: false },
        { e: { kind: 'internal', message: 'boom' }, title: '桌面端内部出错', detail: 'boom', channelIssue: false },
    ] satisfies Array<{ e: DebugError; title: string; detail?: string; channelIssue: boolean }>;

    it.each(CASES)('$e.kind', ({ e, title, detail, channelIssue }) => {
        const copy = debugErrorCopy(e);
        expect(copy.title).toBe(title);
        expect(copy.channelIssue).toBe(channelIssue);
        if (detail !== undefined) expect(copy.detail).toBe(detail);
    });

    it('覆盖了 DebugError 的全部 kind', () => {
        expect(new Set(CASES.map((c) => c.e.kind)).size).toBe(12);
    });

    it('超时秒数：整数不带小数，非整数留一位', () => {
        expect(debugErrorCopy({ kind: 'timeout', ms: 1500 }).detail).toContain('已等待 1.5 秒');
        expect(debugErrorCopy({ kind: 'timeout', ms: 30000 }).detail).toContain('已等待 30 秒');
    });

    it('通道用不了 / 上游太老给「去哪解决」的出口（规格 §3.2 / §3.4），其他 kind 没有', () => {
        expect(debugErrorCopy({ kind: 'channel_unavailable', reason: '端口没映射' }).exits).toEqual(NO_CHANNEL_EXITS);
        expect(debugErrorCopy({ kind: 'upstream_too_old' }).exits).toEqual([UPGRADE_RUNTIME_EXIT]);
        expect(debugErrorCopy({ kind: 'transport', message: 'x' }).exits).toBeUndefined();
        expect(debugErrorCopy({ kind: 'auth_failed', status: 401 }).exits).toBeUndefined();
    });
});

describe('retcodeHint', () => {
    it('常见 retcode 有人话提示', () => {
        expect(retcodeHint(1400)).toContain('参数不对');
        expect(retcodeHint(400)).toContain('参数不对');
        expect(retcodeHint(1401)).toContain('权限不足');
        expect(retcodeHint(1404)).toContain('接口不存在');
        expect(retcodeHint(1403)).not.toBeNull();
        for (const code of [1200, 200, 100]) expect(retcodeHint(code)).toContain('执行出错');
    });

    it('0 和不认识的返回 null', () => {
        expect(retcodeHint(0)).toBeNull();
        expect(retcodeHint(-1)).toBeNull();
        expect(retcodeHint(9999)).toBeNull();
    });
});

describe('callProblem', () => {
    const outcome = (patch: Record<string, unknown>) => ({
        request_id: 'r1',
        result: {
            kind: 'ok' as const,
            outcome: {
                ok: false,
                status: 'failed',
                retcode: 1200,
                data: null,
                message: '',
                wording: '',
                raw: null,
                elapsed_ms: 3,
                channel: { kind: 'internal' as const },
                size_bytes: 0,
                truncated: false,
                ...patch,
            },
        },
    });

    it('成功返回 null', () => {
        expect(callProblem(outcome({ ok: true, status: 'ok', retcode: 0 }))).toBeNull();
    });

    it('OB11 说失败：retcode 带上游说明（wording 优先，其次 message、retcode 人话）', () => {
        expect(callProblem(outcome({ wording: '该请求已被处理' }))).toBe('retcode 1200 · 该请求已被处理');
        expect(callProblem(outcome({ message: 'msg 兜底' }))).toBe('retcode 1200 · msg 兜底');
        expect(callProblem(outcome({}))).toBe('retcode 1200 · 上游执行出错：具体原因看返回里的 message / wording');
        expect(callProblem(outcome({ retcode: 1400 }))).toBe('retcode 1400 · 参数不对：缺了必填项，或者类型 / 取值不符合要求');
    });

    it('没拿到回包：用错误文案（标题加细节）', () => {
        expect(
            callProblem({ request_id: 'r1', result: { kind: 'err', error: { kind: 'timeout', ms: 60_000 } } }),
        ).toBe('等太久了，调用超时：已等待 60 秒。上游可能还在执行，可以调大超时后重试');
        expect(callProblem({ request_id: 'r1', result: { kind: 'err', error: { kind: 'cancelled' } } })).toBe(
            '已取消：取消只是不再等待回包，上游可能已经执行了',
        );
    });
});
