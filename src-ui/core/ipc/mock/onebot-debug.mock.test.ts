import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DomainEvent } from '../types';
import type { DebugCallOrigin } from '../generated/debug/DebugCallOrigin';
import type { DebugCallOutcome } from '../generated/debug/DebugCallOutcome';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../generated/debug/DebugCallResponse';
import type { DebugChannelId } from '../generated/debug/DebugChannelId';
import type { DebugEvent } from '../generated/debug/DebugEvent';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';
import { emitMockEvent } from './events.mock';
import { onebotDebugMock as mock, resetOnebotDebugMock } from './onebot-debug.mock';

const SL = 'mock-bot-sl';
const NC = 'mock-bot-nc-remote';
const DOCKER = 'mock-bot-nc-docker';

let requestSeq = 0;

function req(
    botId: string,
    action: string,
    params: unknown = {},
    extra: Partial<DebugCallRequest> = {},
): DebugCallRequest {
    return {
        request_id: `req-${++requestSeq}`,
        bot_id: botId,
        channel: { kind: 'auto' },
        action,
        params,
        timeout_ms: null,
        origin: 'editor' satisfies DebugCallOrigin,
        ...extra,
    };
}

/** 发一次调用并把假时钟推过去，拿到回包 */
async function callAndWait(request: DebugCallRequest, waitMs = 1000): Promise<DebugCallResponse> {
    const pending = mock.call(request);
    await vi.advanceTimersByTimeAsync(waitMs);
    return pending;
}

function outcomeOf(response: DebugCallResponse): DebugCallOutcome {
    if (response.result.kind !== 'ok')
        throw new Error(`期望拿到回包，实际是 ${response.result.error.kind}`);
    return response.result.outcome;
}

async function open(botId: string, source: DebugChannelId = { kind: 'auto' }) {
    const batches: DebugEventBatch[] = [];
    const pending = mock.subscribe(botId, source, (b) => batches.push(b));
    await vi.advanceTimersByTimeAsync(400);
    const response = await pending;
    return { batches, response, events: (): DebugEvent[] => batches.flatMap((b) => b.events) };
}

const seqsOf = (events: DebugEvent[]) => events.map((e) => e.seq);
/** 后端的体积都按 UTF-8 字节算 */
const utf8Bytes = (text: string) => new TextEncoder().encode(text).length;
const isHeartbeat = (e: DebugEvent) =>
    e.body.kind === 'ob11' && e.body.payload.meta_event_type === 'heartbeat';

beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-29T08:00:00Z'));
    resetOnebotDebugMock();
});

afterEach(() => {
    resetOnebotDebugMock();
    vi.clearAllTimers();
    vi.useRealTimers();
});

describe('targets 与通道', () => {
    it('给出三个走查场景的 Bot，机器人页预览里的 Bot 也出现在目标里', async () => {
        const pending = mock.targets();
        await vi.advanceTimersByTimeAsync(400);
        const targets = await pending;
        expect(targets.map((t) => t.name)).toEqual([
            '小雪',
            'NapCat 测试号',
            '容器里的 NC',
            'Bot-01',
            'Bot-02',
        ]);
        expect(targets[0]).toMatchObject({
            backend: 'snowluma',
            host: { kind: 'local' },
            running: true,
            qq_id: 2854196310,
        });
        expect(targets[1]).toMatchObject({
            backend: 'napcat',
            host: { kind: 'remote', server_id: 'srv-1' },
            qq_id: 1919810,
        });
        expect(targets[2]).toMatchObject({
            backend: 'napcat',
            host: { kind: 'docker', server_id: 'srv-1' },
            running: false,
        });
        // Bot 卡片上的「调试」靠这两行跳进来：id 就是机器人页那些 Bot 的 id
        expect(targets[3]).toMatchObject({
            backend: 'napcat',
            host: { kind: 'local' },
            bot_id: '10001',
            running: true,
        });
        expect(targets[4]).toMatchObject({
            backend: 'napcat',
            host: { kind: 'local' },
            bot_id: '10002',
            running: false,
        });
    });

    it('机器人页预览里的启停会同步到调试台的目标，停掉时接收器也一起收', async () => {
        const pending = mock.targets();
        await vi.advanceTimersByTimeAsync(400);
        const before = await pending;
        expect(before.find((t) => t.bot_id === '10001')?.running).toBe(true);

        // 先订着 10001 的接收器，再从机器人页把它停掉：往订阅推一条「Bot 已停止」
        const batches: DebugEvent[][] = [];
        const sub = mock.subscribe('10001', { kind: 'auto' }, (batch) =>
            batches.push(batch.events),
        );
        await vi.advanceTimersByTimeAsync(400);
        const { receiver } = await sub;
        expect(receiver.state).toEqual({ state: 'connected' });

        emitMockEvent({
            kind: 'bot_state_changed',
            v: 1,
            snapshot: {
                bot_id: '10001',
                state: 'stopped',
                revision: 2,
                token_generation: 1,
                pending_restart: false,
            },
        } as DomainEvent);
        await vi.advanceTimersByTimeAsync(400);

        const afterPending = mock.targets();
        await vi.advanceTimersByTimeAsync(400);
        const after = await afterPending;
        expect(after.find((t) => t.bot_id === '10001')?.running).toBe(false);
        expect(
            batches
                .flat()
                .some(
                    (e) =>
                        e.body.kind === 'receiver' &&
                        e.body.state.state === 'stopped' &&
                        e.body.state.reason === 'Bot 已停止',
                ),
        ).toBe(true);

        emitMockEvent({
            kind: 'bot_state_changed',
            v: 1,
            snapshot: {
                bot_id: '10001',
                state: 'running',
                revision: 3,
                token_generation: 1,
                pending_restart: false,
            },
        } as DomainEvent);
        await vi.advanceTimersByTimeAsync(400);
        const backPending = mock.targets();
        await vi.advanceTimersByTimeAsync(400);
        expect((await backPending).find((t) => t.bot_id === '10001')?.running).toBe(true);
    });

    it('接收器重建（停止 → 重新接收）不再回带旧时间戳的假历史', async () => {
        const batches: DebugEvent[][] = [];
        const sub = mock.subscribe(SL, { kind: 'auto' }, (batch) => batches.push(batch.events));
        await vi.advanceTimersByTimeAsync(400);
        await sub;
        // 头一次订阅会回放出分钟级的假历史
        expect(batches.flat().filter((e) => e.body.kind === 'ob11').length).toBeGreaterThan(10);

        const stopping = mock.stopReceiver(SL);
        await vi.advanceTimersByTimeAsync(400);
        await stopping;

        const again: DebugEvent[][] = [];
        const resub = mock.subscribe(SL, { kind: 'auto' }, (batch) => again.push(batch.events));
        await vi.advanceTimersByTimeAsync(400);
        const { receiver } = await resub;
        expect(receiver.state).toEqual({ state: 'connected' });
        // 重建只写接收器生命周期行，不难把一分钟前的 ob11 假事件再接上来
        expect(again.flat().filter((e) => e.body.kind === 'ob11')).toHaveLength(0);
    });

    it('每个 Bot 有内部 / HTTP / WS 三条通道，容器里的还多一条不支持的 WS', async () => {
        const local = mock.channels(SL);
        const remote = mock.channels(NC);
        const docker = mock.channels(DOCKER);
        await vi.advanceTimersByTimeAsync(400);
        const [sl, nc, dk] = await Promise.all([local, remote, docker]);

        expect(sl.channels.map((c) => c.label)).toEqual([
            'SnowLuma 内部通道',
            'HTTP · http-default :3000',
            'WS · ws-default :3001',
        ]);
        expect(sl.channels.map((c) => c.status.kind)).toEqual([
            'available',
            'available',
            'unknown',
        ]);
        expect(nc.channels.map((c) => c.status.kind)).toEqual(['available', 'tunneled', 'unknown']);
        expect(nc.channels[1].status).toEqual({ kind: 'tunneled', local_port: 54711 });
        expect(nc.auto_call).toEqual({ kind: 'internal' });
        expect(nc.auto_events).toEqual({ kind: 'internal' });

        expect(dk.channels).toHaveLength(4);
        expect(dk.channels[3]).toMatchObject({
            label: 'WS · ws-8080 :8080',
            status: { kind: 'unsupported', reason: '容器没有映射 8080 端口' },
        });
        // Bot 没在运行时其它通道都是「Bot 未运行」，自动也选不出来
        expect(dk.channels.slice(0, 3).map((c) => c.status.kind)).toEqual([
            'bot_not_running',
            'bot_not_running',
            'bot_not_running',
        ]);
        expect(dk.auto_call).toBeNull();
        expect(dk.auto_events).toBeNull();
    });

    it('令牌只给打码后的提示', async () => {
        const pending = mock.channels(NC);
        await vi.advanceTimersByTimeAsync(400);
        const { channels } = await pending;
        expect(channels[0].token_hint).toBeNull();
        expect(channels[1].token_hint).toBe('nc***01');
    });

    it('测试连通：未知的 WS 400 毫秒后变可用，测完才记住', async () => {
        const ws: DebugChannelId = { kind: 'ws', name: 'ws-default' };
        let info: Awaited<ReturnType<typeof mock.testChannel>> | null = null;
        void mock.testChannel(SL, ws).then((v) => {
            info = v;
        });
        await vi.advanceTimersByTimeAsync(200);
        // 探测还在进行，这时拉通道列表看到的还是旧状态
        const during = mock.channels(SL);
        await vi.advanceTimersByTimeAsync(199);
        expect(info).toBeNull();
        await vi.advanceTimersByTimeAsync(1);
        expect(info).toMatchObject({ status: { kind: 'available' } });
        await vi.advanceTimersByTimeAsync(400);
        expect((await during).channels[2].status.kind).toBe('unknown');

        const after = mock.channels(SL);
        await vi.advanceTimersByTimeAsync(400);
        expect((await after).channels[2].status.kind).toBe('available');
    });

    it('测试连通：远端 HTTP 令牌错，测完才返回并记下 401', async () => {
        const pending = mock.testChannel(NC, { kind: 'http', name: 'http-default' });
        const during = mock.channels(NC);
        await vi.advanceTimersByTimeAsync(400);
        expect((await pending).status).toEqual({ kind: 'auth_failed', status: 401 });
        expect((await during).channels[1].status.kind).toBe('tunneled');
        const after = mock.channels(NC);
        await vi.advanceTimersByTimeAsync(400);
        expect((await after).channels[1].status).toEqual({ kind: 'auth_failed', status: 401 });
    });

    it('测试连通途中被重置：结果作废，不写进重置后的状态', async () => {
        const pending = mock.testChannel(SL, { kind: 'ws', name: 'ws-default' });
        await vi.advanceTimersByTimeAsync(100);
        resetOnebotDebugMock();
        await vi.advanceTimersByTimeAsync(300);
        expect((await pending).status.kind).toBe('unknown');
        const list = mock.channels(SL);
        await vi.advanceTimersByTimeAsync(400);
        expect((await list).channels[2].status.kind).toBe('unknown');
    });

    it('不存在的 Bot 拒绝并给出中文原因', async () => {
        const pending = mock.channels('nope');
        const assertion = expect(pending).rejects.toBe('Bot 不存在');
        await vi.advanceTimersByTimeAsync(400);
        await assertion;
    });
});

describe('接口目录', () => {
    async function catalog(botId: string | null, backend: 'napcat' | 'snowluma') {
        const pending = mock.catalog(botId, backend);
        await vi.advanceTimersByTimeAsync(400);
        return pending;
    }

    it('两个后端各有三十个左右的动作，覆盖全部分类和安全等级', async () => {
        const nc = await catalog(null, 'napcat');
        const sl = await catalog(null, 'snowluma');
        expect(nc.actions.length).toBe(32);
        expect(sl.actions.length).toBe(32);

        const categories = new Set(nc.actions.map((a) => a.category));
        expect([...categories].sort()).toEqual(
            [
                'account',
                'extension',
                'face',
                'file',
                'friend',
                'group_admin',
                'group_info',
                'message',
                'request',
                'stream',
            ].sort(),
        );
        expect(new Set(nc.actions.map((a) => a.safety))).toEqual(
            new Set(['read_only', 'side_effect', 'dangerous']),
        );
        expect(nc.actions.find((a) => a.name === 'upload_file_stream')?.stream).toBe(true);
        expect(nc.actions.filter((a) => a.stream)).toHaveLength(1);
    });

    it('独有的动作只出现在自己的后端，并标出对面没有', async () => {
        const nc = await catalog(null, 'napcat');
        const sl = await catalog(null, 'snowluma');
        expect(nc.actions.find((a) => a.name === 'nc_get_rkey')?.other_backend_present).toBe(false);
        expect(nc.actions.find((a) => a.name === 'get_group_album_list')).toBeUndefined();
        expect(
            sl.actions.find((a) => a.name === 'get_group_album_list')?.other_backend_present,
        ).toBe(false);
        expect(sl.actions.find((a) => a.name === 'nc_get_rkey')).toBeUndefined();
        expect(nc.actions.find((a) => a.name === 'get_login_info')?.other_backend_present).toBe(
            true,
        );
    });

    it('id 类参数：NapCat 是字符串，SnowLuma 是整数，都带 x-ncd-role', async () => {
        const ncSpec = mock.describe(null, 'napcat', 'send_group_msg');
        const slSpec = mock.describe(null, 'snowluma', 'send_group_msg');
        await vi.advanceTimersByTimeAsync(400);
        const [nc, sl] = await Promise.all([ncSpec, slSpec]);
        const prop = (spec: typeof nc, name: string) =>
            ((spec?.params_schema.properties ?? {}) as Record<string, Record<string, unknown>>)[
                name
            ];

        expect(prop(nc, 'group_id')).toMatchObject({ type: 'string', 'x-ncd-role': 'group_id' });
        expect(prop(sl, 'group_id')).toMatchObject({ type: 'integer', 'x-ncd-role': 'group_id' });
        expect(prop(nc, 'message')).toMatchObject({ 'x-ncd-role': 'message' });
        expect(prop(nc, 'auto_escape').anyOf).toEqual([{ type: 'boolean' }, { type: 'string' }]);
        expect(prop(sl, 'auto_escape').type).toBe('boolean');
        expect(nc?.params_schema.required).toEqual(['group_id', 'message']);
        // NapCat 有示例和报错样例，SnowLuma 没有
        expect(nc?.examples.length).toBe(1);
        expect(nc?.error_examples.length).toBeGreaterThan(0);
        expect(sl?.examples).toEqual([]);
        expect(sl?.invariants.length).toBeGreaterThan(0);
    });

    it('两个后端的参数差异由定义直接算出来，id 的字符串 / 整数写法不算差异', async () => {
        const list = await catalog(null, 'napcat');
        // 徽章只看真不兼容：get_group_info 两边一样；get_group_member_list 只多一个仅 NC 的可选参数，
        // 照另一边的写法照样能调通，不带徽章（这条差异只在文档页的对照表里列）
        expect(list.actions.find((a) => a.name === 'get_group_info')?.param_diff).toBe(false);
        expect(list.actions.find((a) => a.name === 'get_group_member_list')?.param_diff).toBe(
            false,
        );
        // set_group_ban 的 duration 一边 number 一边 integer，integer 是 number 的子集，比较前归一，不算差异
        expect(list.actions.find((a) => a.name === 'set_group_ban')?.param_diff).toBe(false);
        // move_group_file 两边把「目标目录」写成不同的必填参数名，照一边写发到另一边必失败，带徽章
        expect(list.actions.find((a) => a.name === 'move_group_file')?.param_diff).toBe(true);

        const members = mock.describe(null, 'napcat', 'get_group_member_list');
        const ban = mock.describe(null, 'napcat', 'set_group_ban');
        const move = mock.describe(null, 'napcat', 'move_group_file');
        const like = mock.describe(null, 'snowluma', 'send_like');
        const upload = mock.describe(null, 'napcat', 'upload_group_file');
        await vi.advanceTimersByTimeAsync(400);
        expect((await members)?.other_backend?.diffs).toEqual([
            { name: 'no_cache', diff: { kind: 'only_here' } },
        ]);
        expect((await members)?.other_backend?.breaking).toBe(false);
        expect((await ban)?.other_backend?.diffs).toEqual([]);
        expect((await ban)?.other_backend?.breaking).toBe(false);
        expect((await move)?.other_backend?.diffs).toEqual([
            { name: 'target_parent_directory', diff: { kind: 'only_here' } },
            { name: 'target_directory', diff: { kind: 'only_other' } },
        ]);
        expect((await move)?.other_backend?.breaking).toBe(true);
        expect((await like)?.other_backend?.diffs).toEqual([
            { name: 'times', diff: { kind: 'required_differs', here: true, other: false } },
        ]);
        expect((await like)?.other_backend?.breaking).toBe(false);
        expect((await upload)?.other_backend?.diffs).toEqual([
            { name: 'folder_id', diff: { kind: 'only_here' } },
            { name: 'folder', diff: { kind: 'only_other' } },
        ]);
        expect((await upload)?.other_backend?.breaking).toBe(false);
    });

    it('运行中的 Bot 用在线目录，缺的动作进「不支持」；没选 Bot 或没在跑就是快照', async () => {
        const live = await catalog(NC, 'napcat');
        expect(live.source).toBe('live');
        expect(live.actions.filter((a) => !a.supported).map((a) => a.name)).toEqual([
            'fetch_custom_face',
        ]);
        expect((await catalog(null, 'napcat')).source).toBe('snapshot');
        expect((await catalog(DOCKER, 'napcat')).source).toBe('snapshot');
    });

    it('预览专用的 debug_huge_response：两个后端都有，只读、无参数', async () => {
        const nc = await catalog(NC, 'napcat');
        const sl = await catalog(SL, 'snowluma');
        for (const list of [nc, sl]) {
            expect(list.actions.find((a) => a.name === 'debug_huge_response')).toMatchObject({
                summary: '预览用：回一个超过 5 MiB 的回包',
                category: 'extension',
                safety: 'read_only',
                supported: true,
                other_backend_present: true,
                param_diff: false,
            });
        }
        const spec = mock.describe(null, 'napcat', 'debug_huge_response');
        await vi.advanceTimersByTimeAsync(400);
        expect((await spec)?.params_schema).toEqual({ type: 'object', properties: {} });
    });

    it('别名也能查到动作，没有的返回 null', async () => {
        const byAlias = mock.describe(null, 'napcat', 'recall_msg');
        const missing = mock.describe(null, 'napcat', 'no_such_action');
        await vi.advanceTimersByTimeAsync(400);
        expect((await byAlias)?.name).toBe('delete_msg');
        expect(await missing).toBeNull();
    });
});

describe('调用', () => {
    it('群列表 40 个、好友 60 个、群 1 的成员 500 个', async () => {
        const groups = outcomeOf(await callAndWait(req(NC, 'get_group_list')));
        expect(groups.ok).toBe(true);
        const list = groups.data as Array<{ group_id: number; group_name: string }>;
        expect(list).toHaveLength(40);
        expect(list[0]).toMatchObject({ group_id: 100001, group_name: '测试群 1' });
        expect(list[39].group_name).toBe('测试群 40');
        expect(groups.channel).toEqual({ kind: 'internal' });

        const friends = outcomeOf(await callAndWait(req(SL, 'get_friend_list')));
        expect(friends.data as unknown[]).toHaveLength(60);

        const members = outcomeOf(
            await callAndWait(req(NC, 'get_group_member_list', { group_id: '100001' })),
        );
        expect(members.data as unknown[]).toHaveLength(500);
    });

    it('群成员列表要 900 毫秒', async () => {
        let response: DebugCallResponse | null = null;
        void mock.call(req(NC, 'get_group_member_list', { group_id: '100001' })).then((v) => {
            response = v;
        });
        await vi.advanceTimersByTimeAsync(899);
        expect(response).toBeNull();
        await vi.advanceTimersByTimeAsync(1);
        expect(response).not.toBeNull();
    });

    it('其它调用 60 到 400 毫秒内回', async () => {
        for (let i = 0; i < 6; i += 1) {
            let done = false;
            void mock.call(req(SL, 'get_status')).then(() => {
                done = true;
            });
            await vi.advanceTimersByTimeAsync(400);
            expect(done).toBe(true);
        }
    });

    it('回包是完整的 OB11 回复；NapCat 多一个 stream 字段，回包带 echo', async () => {
        const request = req(NC, 'get_login_info');
        const outcome = outcomeOf(await callAndWait(request));
        expect(outcome).toMatchObject({
            ok: true,
            status: 'ok',
            retcode: 0,
            data: { user_id: 1919810, nickname: 'NapCat 测试号' },
        });
        expect(outcome.raw).toMatchObject({
            status: 'ok',
            retcode: 0,
            echo: request.request_id,
            stream: 'normal-action',
        });
        expect(outcome.size_bytes).toBeGreaterThan(20);
        expect(outcome.truncated).toBe(false);
        const sl = outcomeOf(await callAndWait(req(SL, 'get_login_info')));
        expect(sl.raw).not.toHaveProperty('stream');
    });

    it('目录外的动作：拿到回包，ok:false，retcode 1404', async () => {
        const outcome = outcomeOf(await callAndWait(req(SL, 'no_such_action')));
        expect(outcome).toMatchObject({ ok: false, status: 'failed', retcode: 1404 });
        expect(outcome.wording).toContain('no_such_action');
    });

    it('另一个后端才有的动作，在这个 Bot 上也是 1404', async () => {
        expect(outcomeOf(await callAndWait(req(SL, 'nc_get_rkey'))).retcode).toBe(1404);
        expect(outcomeOf(await callAndWait(req(NC, 'nc_get_rkey'))).retcode).toBe(0);
        // 老版本缺的动作也一样
        expect(outcomeOf(await callAndWait(req(NC, 'fetch_custom_face'))).retcode).toBe(1404);
        expect(outcomeOf(await callAndWait(req(SL, 'fetch_custom_face'))).retcode).toBe(0);
    });

    it('缺必填参数是 1400；群不存在是 1200', async () => {
        const missing = outcomeOf(await callAndWait(req(NC, 'send_group_msg', { message: 'hi' })));
        expect(missing).toMatchObject({ ok: false, retcode: 1400 });
        expect(missing.wording).toContain('group_id');
        const noGroup = outcomeOf(
            await callAndWait(req(NC, 'get_group_info', { group_id: '999' })),
        );
        expect(noGroup).toMatchObject({ ok: false, retcode: 1200 });
    });

    it('get_msg / get_group_member_info 的成败', async () => {
        const member = outcomeOf(
            await callAndWait(
                req(SL, 'get_group_member_info', { group_id: 100001, user_id: 10002 }),
            ),
        );
        expect(member.data).toMatchObject({ user_id: 10002, group_id: 100001 });
        expect(outcomeOf(await callAndWait(req(SL, 'get_msg', { message_id: 1 }))).retcode).toBe(
            1200,
        );
        expect(
            outcomeOf(
                await callAndWait(
                    req(SL, 'set_group_ban', { group_id: 100001, user_id: 10002, duration: 60 }),
                ),
            ).ok,
        ).toBe(true);
    });

    it('流式接口和坏参数是没拿到回包的错误', async () => {
        const stream = await callAndWait(req(NC, 'upload_file_stream', { stream_id: 's1' }));
        expect(stream.result).toMatchObject({ kind: 'err', error: { kind: 'invalid_params' } });
        const bad = await callAndWait(req(NC, 'get_status', 'oops'));
        expect(bad.result).toMatchObject({ kind: 'err', error: { kind: 'invalid_params' } });
    });

    it('Bot 没在运行、Bot 不存在、令牌错的通道都返回对应的错', async () => {
        const stopped = await callAndWait(req(DOCKER, 'get_status'), 200);
        expect(stopped.result).toEqual({ kind: 'err', error: { kind: 'bot_not_running' } });
        const missing = await callAndWait(req('nope', 'get_status'), 200);
        expect(missing.result).toEqual({ kind: 'err', error: { kind: 'bot_not_found' } });
        const http = await callAndWait(
            req(NC, 'get_status', {}, { channel: { kind: 'http', name: 'http-default' } }),
            200,
        );
        expect(http.result).toEqual({ kind: 'err', error: { kind: 'auth_failed', status: 401 } });
        const unsupported = await callAndWait(
            req(DOCKER, 'get_status', {}, { channel: { kind: 'ws', name: 'ws-8080' } }),
            200,
        );
        expect(unsupported.result).toMatchObject({ kind: 'err' });
    });

    it('timeout_ms 有效：比响应时间短就超时', async () => {
        const response = await callAndWait(
            req(NC, 'get_group_member_list', { group_id: '100001' }, { timeout_ms: 100 }),
            150,
        );
        expect(response.result).toEqual({ kind: 'err', error: { kind: 'timeout', ms: 100 } });
        // 超时之后活干不完了，不该留着定时器
        expect(vi.getTimerCount()).toBe(0);
    });

    it('cancel：进行中的调用立刻以 Cancelled 结束，上游动作不再执行', async () => {
        const events = await open(NC);
        const request = req(NC, 'send_group_msg', { group_id: '100001', message: '取消我' });
        const pending = mock.call(request);
        await vi.advanceTimersByTimeAsync(10);
        await mock.cancel(request.request_id);
        expect(await pending).toEqual({
            request_id: request.request_id,
            result: { kind: 'err', error: { kind: 'cancelled' } },
        });

        await vi.advanceTimersByTimeAsync(1000);
        const sent = events
            .events()
            .filter((e) => e.body.kind === 'ob11' && e.body.payload.post_type === 'message_sent');
        expect(sent).toHaveLength(0);
        // 取消也会在事件流里留一条失败的调用记录
        const record = events.events().find((e) => e.body.kind === 'call');
        expect(record?.body).toMatchObject({
            kind: 'call',
            record: { request_id: request.request_id, ok: false, retcode: null, error: '已取消' },
        });
        await mock.unsubscribe(events.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('对已经结束或不存在的请求 cancel 不出错', async () => {
        await expect(mock.cancel('no-such-request')).resolves.toBeUndefined();
    });

    it('send_group_msg 的 message_id 递增', async () => {
        const first = outcomeOf(
            await callAndWait(req(SL, 'send_group_msg', { group_id: 100001, message: 'a' })),
        );
        const second = outcomeOf(
            await callAndWait(req(SL, 'send_private_msg', { user_id: 10001, message: 'b' })),
        );
        const a = (first.data as { message_id: number }).message_id;
        const b = (second.data as { message_id: number }).message_id;
        expect(b).toBe(a + 1);
    });

    it('五百人的群成员列表远不到 5 MiB：不截断，data 和 raw 都是完整的', async () => {
        const outcome = outcomeOf(
            await callAndWait(req(NC, 'get_group_member_list', { group_id: '100001' })),
        );
        expect(outcome.size_bytes).toBeGreaterThan(100_000);
        expect(outcome.truncated).toBe(false);
        expect(outcome.data as unknown[]).toHaveLength(500);
        expect(outcome.raw).toMatchObject({ status: 'ok', retcode: 0 });
    });

    it('回包超过 5 MiB 才截断：data 置空，raw 换成不超过 256 KiB 的文本预览，头部字段照常', async () => {
        for (const botId of [SL, NC]) {
            const request = req(botId, 'debug_huge_response');
            const outcome = outcomeOf(await callAndWait(request));
            expect(outcome).toMatchObject({
                ok: true,
                status: 'ok',
                retcode: 0,
                message: '',
                wording: '',
                truncated: true,
            });
            expect(outcome.data).toBeNull();
            expect(outcome.size_bytes).toBeGreaterThan(5 * 1024 * 1024);
            expect(typeof outcome.raw).toBe('string');
            const preview = outcome.raw as string;
            const bytes = utf8Bytes(preview);
            // 最多 256 KiB，只会因为不切在多字节字符中间往回退几个字节
            expect(bytes).toBeLessThanOrEqual(256 * 1024);
            expect(bytes).toBeGreaterThan(256 * 1024 - 4);
            expect(preview.startsWith('{"status":"ok","retcode":0,"data":{"note":')).toBe(true);
            // 预览停在字符边界上：最后一个码元不是半个代理对
            const last = preview.charCodeAt(preview.length - 1);
            expect(last >= 0xd800 && last <= 0xdbff).toBe(false);
        }
    });

    it('saveResponse 只认最近 3 次被截断的调用，没截断的回包不留全文', async () => {
        const normal = req(NC, 'get_status');
        await callAndWait(normal);
        const huge: DebugCallRequest[] = [];
        for (let i = 0; i < 4; i += 1) {
            const request = req(i % 2 === 0 ? SL : NC, 'debug_huge_response');
            await callAndWait(request);
            huge.push(request);
        }
        const reason = '没有这次调用的完整回包：只保留最近 3 次被截断的回包';
        const saves = huge.map((r) =>
            mock.saveResponse(r.request_id, `C:/tmp/${r.request_id}.json`),
        );
        const rejected = [
            expect(mock.saveResponse(normal.request_id, 'C:/tmp/out.json')).rejects.toBe(reason),
            expect(mock.saveResponse('unknown', 'C:/tmp/out.json')).rejects.toBe(reason),
            // 第一次的全文已经被后面三次挤掉了
            expect(saves[0]).rejects.toBe(reason),
        ];
        await vi.advanceTimersByTimeAsync(400);
        await Promise.all(rejected);
        for (const save of saves.slice(1)) await expect(save).resolves.toBeUndefined();
    });
});

describe('事件流', () => {
    it('订阅先补缓冲，再推实时事件，seq 严格递增且不重复', async () => {
        const stream = await open(NC);
        expect(stream.response.subscription_id).toMatch(/^mock-sub-/);
        expect(stream.response.receiver).toMatchObject({
            bot_id: NC,
            source: { kind: 'internal' },
            state: { state: 'connected' },
            viewers: 1,
        });

        const backlog = stream.events();
        expect(backlog.length).toBeGreaterThan(10);
        expect(backlog[0].body).toMatchObject({ kind: 'receiver', state: { state: 'connecting' } });
        expect(
            backlog.some(
                (e) => e.body.kind === 'ob11' && e.body.payload.meta_event_type === 'lifecycle',
            ),
        ).toBe(true);
        // 补给它的时候不该有定时器在等（实时推送还没到点）
        const backlogCount = backlog.length;

        await vi.advanceTimersByTimeAsync(20_000);
        const all = stream.events();
        expect(all.length).toBeGreaterThan(backlogCount + 3);
        const seqs = seqsOf(all);
        expect(seqs).toEqual([...new Set(seqs)].sort((a, b) => a - b));
        expect(seqs[0]).toBe(1);
        expect(seqs[seqs.length - 1]).toBe(seqs.length);
        // 时间戳不倒退
        const stamps = all.map((e) => e.at_ms);
        expect(stamps).toEqual([...stamps].sort((a, b) => a - b));
        for (const batch of stream.batches) {
            expect(batch).toMatchObject({ v: 1, bot_id: NC });
        }

        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('实时事件每 1.2–3 秒一条，并且 50 毫秒合一批', async () => {
        const stream = await open(SL);
        const before = stream.events().length;
        const batchesBefore = stream.batches.length;
        await vi.advanceTimersByTimeAsync(1199);
        expect(stream.events().length).toBe(before);
        await vi.advanceTimersByTimeAsync(1851);
        // 最迟 3 秒一定来了一条，加 50 毫秒的合批窗口
        expect(stream.events().length).toBeGreaterThan(before);
        expect(stream.batches.length).toBeGreaterThan(batchesBefore);
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('30 秒左右来一条心跳', async () => {
        const stream = await open(SL);
        const seqBefore = stream.events().length;
        await vi.advanceTimersByTimeAsync(30_100);
        const live = stream.events().slice(seqBefore);
        const beats = live.filter(
            (e) => e.body.kind === 'ob11' && e.body.payload.meta_event_type === 'heartbeat',
        );
        expect(beats).toHaveLength(1);
        expect(beats[0].body).toMatchObject({
            kind: 'ob11',
            payload: { self_id: 2854196310, interval: 30000 },
        });
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('NapCat 的内部通道不发心跳，SnowLuma 照旧', async () => {
        const nc = await open(NC, { kind: 'internal' });
        const sl = await open(SL, { kind: 'internal' });
        const ncBefore = nc.events().length;
        const slBefore = sl.events().length;
        await vi.advanceTimersByTimeAsync(60_000);
        expect(nc.events().slice(ncBefore).filter(isHeartbeat)).toHaveLength(0);
        expect(sl.events().slice(slBefore).filter(isHeartbeat)).toHaveLength(2);
        // 普通事件照常在来，只是没有心跳
        expect(nc.events().length).toBeGreaterThan(ncBefore + 10);
        await mock.unsubscribe(nc.response.subscription_id);
        await mock.unsubscribe(sl.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('换事件来源时心跳跟着开关：NapCat 换到 WS 有心跳，换回内部通道就停', async () => {
        const internal = await open(NC, { kind: 'internal' });
        await vi.advanceTimersByTimeAsync(40_000);
        expect(internal.events().filter(isHeartbeat)).toHaveLength(0);

        const ws = await open(NC, { kind: 'ws', name: 'ws-default' });
        const atSwitch = internal.events().length;
        await vi.advanceTimersByTimeAsync(30_100);
        // 接收器是一个，换了来源以后两个窗口都收到这一条心跳
        expect(internal.events().slice(atSwitch).filter(isHeartbeat)).toHaveLength(1);

        const back = await open(NC, { kind: 'internal' });
        const atBack = internal.events().length;
        await vi.advanceTimersByTimeAsync(90_000);
        expect(internal.events().slice(atBack).filter(isHeartbeat)).toHaveLength(0);

        for (const s of [internal, ws, back]) await mock.unsubscribe(s.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('每 60 秒给每个窗口发一个空批次探活；退订、停接收器、重置后都不再发', async () => {
        const a = await open(SL);
        const b = await open(SL);
        const probes = (s: typeof a) => s.batches.filter((batch) => batch.events.length === 0);
        await vi.advanceTimersByTimeAsync(59_000);
        expect(probes(a)).toHaveLength(0);
        await vi.advanceTimersByTimeAsync(1_000);
        expect(probes(a)).toEqual([{ v: 1, bot_id: SL, events: [] }]);
        expect(probes(b)).toHaveLength(1);

        // 退掉一个，剩下那个照样按分钟收
        await mock.unsubscribe(a.response.subscription_id);
        await vi.advanceTimersByTimeAsync(60_000);
        expect(probes(a)).toHaveLength(1);
        expect(probes(b)).toHaveLength(2);

        // 接收器停了，定时器全清，不再探
        const stopping = mock.stopReceiver(SL);
        await vi.advanceTimersByTimeAsync(400);
        await stopping;
        expect(vi.getTimerCount()).toBe(0);
        await vi.advanceTimersByTimeAsync(120_000);
        expect(probes(b)).toHaveLength(2);

        // 重置也把探活的定时器清掉
        const c = await open(NC);
        resetOnebotDebugMock();
        expect(vi.getTimerCount()).toBe(0);
        await vi.advanceTimersByTimeAsync(120_000);
        expect(probes(c)).toHaveLength(0);
    });

    it('两个后端各用自己的事件形状：NapCat 消息带 raw 和 message_format，SnowLuma 没有', async () => {
        const collect = async (botId: string) => {
            const stream = await open(botId);
            await vi.advanceTimersByTimeAsync(60_000);
            await mock.unsubscribe(stream.response.subscription_id);
            return stream.events().flatMap((e) => (e.body.kind === 'ob11' ? [e.body.payload] : []));
        };
        const nc = await collect(NC);
        const sl = await collect(SL);
        const ncMessages = nc.filter((p) => p.post_type === 'message');
        const slMessages = sl.filter((p) => p.post_type === 'message');
        expect(ncMessages.length).toBeGreaterThan(5);
        expect(slMessages.length).toBeGreaterThan(5);
        expect(
            ncMessages.every((p) => typeof p.raw === 'object' && p.message_format === 'array'),
        ).toBe(true);
        expect(slMessages.every((p) => !('raw' in p) && !('message_format' in p))).toBe(true);
        expect(ncMessages.every((p) => p.self_id === 1919810)).toBe(true);
        expect(slMessages.every((p) => p.self_id === 2854196310)).toBe(true);
    });

    it('内容是日常的一份混合：群消息带各种消息段，还有通知和请求', async () => {
        const stream = await open(NC);
        await vi.advanceTimersByTimeAsync(300_000);
        await mock.unsubscribe(stream.response.subscription_id);
        const payloads = stream
            .events()
            .flatMap((e) => (e.body.kind === 'ob11' ? [e.body.payload] : []));

        const groupMessages = payloads.filter(
            (p) => p.post_type === 'message' && p.message_type === 'group',
        );
        const segmentTypes = new Set(
            groupMessages.flatMap((p) => (p.message as Array<{ type: string }>).map((s) => s.type)),
        );
        for (const type of ['text', 'at', 'reply', 'image', 'face'])
            expect(segmentTypes).toContain(type);
        // 五个人、三个群
        expect(new Set(groupMessages.map((p) => p.user_id)).size).toBeLessThanOrEqual(5);
        expect(new Set(groupMessages.map((p) => p.group_id)).size).toBe(3);
        expect(
            payloads.some((p) => p.post_type === 'message' && p.message_type === 'private'),
        ).toBe(true);
        const noticeTypes = new Set(
            payloads.filter((p) => p.post_type === 'notice').map((p) => p.notice_type),
        );
        for (const type of ['group_increase', 'group_recall', 'notify'])
            expect(noticeTypes).toContain(type);
        expect(payloads.some((p) => p.post_type === 'request' && p.request_type === 'friend')).toBe(
            true,
        );
        // 回复引用的是真实存在过的群消息
        const ids = new Set(groupMessages.map((p) => String(p.message_id)));
        const replies = groupMessages.flatMap((p) =>
            (p.message as Array<{ type: string; data: { id?: string } }>).filter(
                (s) => s.type === 'reply',
            ),
        );
        expect(replies.length).toBeGreaterThan(0);
        expect(replies.every((s) => ids.has(String(s.data.id)))).toBe(true);
    });

    it('退订到零后所有定时器都清掉，再订阅时按空档补一批带真实时间的事件', async () => {
        const stream = await open(NC);
        await vi.advanceTimersByTimeAsync(5000);
        await mock.unsubscribe(stream.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
        const lastSeq = seqsOf(stream.events()).at(-1) ?? 0;

        // 没人看的一分钟，假时钟里什么都不会发生
        await vi.advanceTimersByTimeAsync(60_000);
        expect(seqsOf(stream.events()).at(-1)).toBe(lastSeq);

        const again = await open(NC);
        const events = again.events();
        expect(events.length).toBeGreaterThan(lastSeq);
        // 补出来的事件夹在两次订阅之间
        const between = events.filter((e) => e.seq > lastSeq && e.at_ms < Date.now() - 300);
        expect(between.length).toBeGreaterThan(5);
        expect(seqsOf(events)).toEqual([...new Set(seqsOf(events))]);
        await mock.unsubscribe(again.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('两个订阅者：退一个接收器还在，退到零才停；退订不认识的订阅号不出错', async () => {
        const a = await open(NC);
        const b = await open(NC);
        expect(b.response.receiver.viewers).toBe(2);
        // 第二个订阅者拿到的缓冲里已有第一个订阅者看到的所有事件
        expect(seqsOf(b.events()).length).toBeGreaterThanOrEqual(seqsOf(a.events()).length);

        await mock.unsubscribe(a.response.subscription_id);
        expect(vi.getTimerCount()).toBeGreaterThan(0);
        await vi.advanceTimersByTimeAsync(4000);
        expect(b.events().length).toBeGreaterThan(seqsOf(a.events()).length);

        await mock.unsubscribe(b.response.subscription_id);
        await mock.unsubscribe(b.response.subscription_id);
        await mock.unsubscribe('mock-sub-unknown');
        expect(vi.getTimerCount()).toBe(0);
    });

    it('没在运行的 Bot、不能收事件的通道、不存在的 Bot 订阅不上，抛出的是中文字符串', async () => {
        const stopped = mock.subscribe(DOCKER, { kind: 'auto' }, () => {});
        const http = mock.subscribe(NC, { kind: 'http', name: 'http-default' }, () => {});
        const nobody = mock.subscribe('nope', { kind: 'auto' }, () => {});
        const assertions = [
            expect(stopped).rejects.toBe('Bot 没有在运行'),
            expect(http).rejects.toBe('通道不可用：这条通道不能接收事件'),
            expect(nobody).rejects.toBe('Bot 不存在'),
        ];
        await vi.advanceTimersByTimeAsync(400);
        await Promise.all(assertions);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('换事件来源：缓冲保留，多两条接收器状态', async () => {
        const first = await open(NC, { kind: 'internal' });
        const seqBefore = seqsOf(first.events()).length;
        const second = await open(NC, { kind: 'ws', name: 'ws-default' });
        expect(second.response.receiver.source).toEqual({ kind: 'ws', name: 'ws-default' });
        expect(seqsOf(second.events()).length).toBe(seqBefore + 2);
        expect(
            second
                .events()
                .slice(-2)
                .map((e) => e.body),
        ).toMatchObject([
            {
                kind: 'receiver',
                state: { state: 'connecting' },
                source: { kind: 'ws', name: 'ws-default' },
            },
            {
                kind: 'receiver',
                state: { state: 'connected' },
                source: { kind: 'ws', name: 'ws-default' },
            },
        ]);
        await mock.unsubscribe(first.response.subscription_id);
        await mock.unsubscribe(second.response.subscription_id);
    });

    it('receivers 列出正在收的 Bot；stopReceiver 推一条 stopped 后清掉缓冲和定时器', async () => {
        const stream = await open(NC);
        const listed = mock.receivers();
        await vi.advanceTimersByTimeAsync(400);
        const infos = await listed;
        expect(infos).toHaveLength(1);
        expect(infos[0]).toMatchObject({ bot_id: NC, viewers: 1, dropped_total: 0, first_seq: 1 });
        expect(infos[0].buffered).toBe(stream.events().length);

        const stopping = mock.stopReceiver(NC);
        await vi.advanceTimersByTimeAsync(400);
        await stopping;
        expect(stream.events().at(-1)?.body).toMatchObject({
            kind: 'receiver',
            state: { state: 'stopped', reason: '已手动停止' },
        });
        expect(vi.getTimerCount()).toBe(0);
        const after = mock.receivers();
        await vi.advanceTimersByTimeAsync(400);
        expect(await after).toEqual([]);

        // 停了以后重新订阅是一个全新的接收器，seq 不回头
        const lastSeq = seqsOf(stream.events()).at(-1) ?? 0;
        const again = await open(NC);
        expect(again.events()[0].seq).toBeGreaterThan(lastSeq);
        await mock.unsubscribe(again.response.subscription_id);
    });

    it('readEvents 按游标读缓冲：seq 大于 sinceSeq 的前 limit 条', async () => {
        const stream = await open(SL);
        await vi.advanceTimersByTimeAsync(10_000);
        // 先退订让缓冲定住，再读；否则读的这 400 毫秒里又来了新事件
        await mock.unsubscribe(stream.response.subscription_id);
        const wait = async <T>(p: Promise<T>) => {
            await vi.advanceTimersByTimeAsync(400);
            return p;
        };
        const all = seqsOf(await wait(mock.readEvents(SL, 0, 100_000)));
        const total = all.length;
        expect(total).toBeGreaterThan(15);
        expect(all).toEqual(Array.from({ length: total }, (_, i) => i + 1));

        expect(seqsOf(await wait(mock.readEvents(SL, 5, 10)))).toEqual([
            6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        ]);
        expect(seqsOf(await wait(mock.readEvents(SL, total - 2, 100)))).toEqual([total - 1, total]);
        expect(await wait(mock.readEvents(SL, total, 100))).toEqual([]);
    });

    it('同样的种子、同样的操作序列，得到完全一样的事件', async () => {
        const transcript = async () => {
            vi.setSystemTime(new Date('2026-09-29T08:00:00Z'));
            const stream = await open(NC);
            await vi.advanceTimersByTimeAsync(20_000);
            await mock.unsubscribe(stream.response.subscription_id);
            return JSON.stringify(stream.events());
        };
        const first = await transcript();
        resetOnebotDebugMock();
        expect(await transcript()).toBe(first);
    });
});

describe('调用和事件流联动', () => {
    it('send_group_msg：回包里的 message_id 就是事件流里 message_sent 的 message_id，另有一条调用记录', async () => {
        const stream = await open(NC);
        const request = req(NC, 'send_group_msg', {
            group_id: '100001',
            message: [{ type: 'text', data: { text: '你好，调试台' } }],
        });
        const response = await callAndWait(request);
        const messageId = (outcomeOf(response).data as { message_id: number }).message_id;
        await vi.advanceTimersByTimeAsync(100);

        const events = stream.events();
        const sent = events.find(
            (e) =>
                e.body.kind === 'ob11' &&
                e.body.payload.post_type === 'message_sent' &&
                e.body.payload.message_id === messageId,
        );
        expect(sent).toBeDefined();
        expect(sent?.body).toMatchObject({
            kind: 'ob11',
            payload: {
                post_type: 'message_sent',
                message_type: 'group',
                group_id: 100001,
                user_id: 1919810,
                self_id: 1919810,
                message: [{ type: 'text', data: { text: '你好，调试台' } }],
                raw_message: '你好，调试台',
            },
        });
        const call = events.find(
            (e) => e.body.kind === 'call' && e.body.record.request_id === request.request_id,
        );
        expect(call?.body).toMatchObject({
            kind: 'call',
            record: {
                action: 'send_group_msg',
                origin: 'editor',
                ok: true,
                retcode: 0,
                message_id: messageId,
                error: null,
                channel: { kind: 'internal' },
            },
        });
        // message_sent 先到，调用记录随回包到，界面两种顺序都要能合并
        expect((sent?.seq ?? 0) < (call?.seq ?? 0)).toBe(true);
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('字符串消息里的 CQ 码会被拆成消息段；私聊 message_sent 带 target_id；SnowLuma 的 message_sent 没有 raw', async () => {
        const stream = await open(SL);
        const response = await callAndWait(
            req(SL, 'send_private_msg', { user_id: 10001, message: '[CQ:at,qq=10002]收到' }),
        );
        const messageId = (outcomeOf(response).data as { message_id: number }).message_id;
        await vi.advanceTimersByTimeAsync(100);
        const sent = stream
            .events()
            .find((e) => e.body.kind === 'ob11' && e.body.payload.message_id === messageId);
        expect(sent?.body).toMatchObject({
            payload: {
                post_type: 'message_sent',
                message_type: 'private',
                target_id: 10001,
                message: [
                    { type: 'at', data: { qq: '10002' } },
                    { type: 'text', data: { text: '收到' } },
                ],
            },
        });
        expect(sent?.body.kind === 'ob11' && 'raw' in sent.body.payload).toBe(false);
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('失败的调用也在事件流里留记录，带原因', async () => {
        const stream = await open(NC);
        await callAndWait(
            req(NC, 'get_group_member_list', { group_id: '100001' }, { timeout_ms: 100 }),
            150,
        );
        const failed = await callAndWait(
            req(NC, 'send_group_msg', { group_id: '999', message: 'x' }),
        );
        expect(outcomeOf(failed).retcode).toBe(1200);
        await vi.advanceTimersByTimeAsync(100);
        const records = stream
            .events()
            .flatMap((e) => (e.body.kind === 'call' ? [e.body.record] : []));
        expect(records).toHaveLength(2);
        expect(records[0]).toMatchObject({ ok: false, retcode: null, error: '等待超过 100 毫秒' });
        expect(records[1]).toMatchObject({
            ok: false,
            retcode: 1200,
            message_id: null,
            error: null,
        });
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('没有接收器时调用不会往事件流里写东西，但发出去的消息 get_msg 还查得到', async () => {
        const sent = outcomeOf(
            await callAndWait(req(NC, 'send_group_msg', { group_id: '100001', message: '悄悄发' })),
        );
        const messageId = (sent.data as { message_id: number }).message_id;
        const receivers = mock.receivers();
        await vi.advanceTimersByTimeAsync(400);
        expect(await receivers).toEqual([]);
        const got = outcomeOf(
            await callAndWait(req(NC, 'get_msg', { message_id: String(messageId) })),
        );
        expect(got).toMatchObject({
            ok: true,
            data: { message_id: messageId, message_type: 'group', group_id: 100001 },
        });
    });

    it('delete_msg 会推一条撤回通知；禁言 / 踢人也推对应通知', async () => {
        const stream = await open(SL);
        const sent = outcomeOf(
            await callAndWait(req(SL, 'send_group_msg', { group_id: 100002, message: '马上撤回' })),
        );
        const messageId = (sent.data as { message_id: number }).message_id;
        expect(
            outcomeOf(await callAndWait(req(SL, 'delete_msg', { message_id: messageId }))).ok,
        ).toBe(true);
        expect(
            outcomeOf(await callAndWait(req(SL, 'delete_msg', { message_id: messageId }))).retcode,
        ).toBe(1200);
        await callAndWait(
            req(SL, 'set_group_ban', { group_id: 100001, user_id: 10002, duration: 0 }),
        );
        await callAndWait(req(SL, 'set_group_kick', { group_id: 100001, user_id: 10003 }));
        await vi.advanceTimersByTimeAsync(100);

        const notices = stream
            .events()
            .flatMap((e) =>
                e.body.kind === 'ob11' && e.body.payload.post_type === 'notice'
                    ? [e.body.payload]
                    : [],
            );
        expect(notices).toContainEqual(
            expect.objectContaining({
                notice_type: 'group_recall',
                message_id: messageId,
                group_id: 100002,
                operator_id: 2854196310,
            }),
        );
        expect(notices).toContainEqual(
            expect.objectContaining({
                notice_type: 'group_ban',
                sub_type: 'lift_ban',
                user_id: 10002,
            }),
        );
        expect(notices).toContainEqual(
            expect.objectContaining({
                notice_type: 'group_decrease',
                sub_type: 'kick',
                user_id: 10003,
            }),
        );
        await mock.unsubscribe(stream.response.subscription_id);
    });

    it('__ncdDebugFlood：一口气灌进正在被看的 Bot，一批送到；超过缓冲上限时丢最旧的并写 dropped', async () => {
        const flood = (
            window as unknown as { __ncdDebugFlood?: (n: number, botId?: string) => number }
        ).__ncdDebugFlood;
        expect(flood).toBeTypeOf('function');
        // 没人看的时候什么都不灌
        expect(flood?.(10)).toBe(0);

        const stream = await open(SL);
        const before = stream.events().length;
        expect(flood?.(300, SL)).toBe(300);
        await vi.advanceTimersByTimeAsync(60);
        expect(stream.events().length).toBe(before + 300);
        expect(stream.batches.at(-1)?.events).toHaveLength(300);

        expect(flood?.(5200)).toBe(5200);
        await vi.advanceTimersByTimeAsync(60);
        const info = mock.receivers();
        await vi.advanceTimersByTimeAsync(400);
        const [receiver] = await info;
        expect(receiver.buffered).toBeLessThan(5100);
        expect(receiver.dropped_total).toBeGreaterThan(0);
        expect(receiver.first_seq).toBeGreaterThan(1);
        const dropped = stream.events().filter((e) => e.body.kind === 'dropped');
        expect(dropped.length).toBeGreaterThan(0);
        expect(
            dropped.reduce((n, e) => n + (e.body.kind === 'dropped' ? e.body.count : 0), 0),
        ).toBe(receiver.dropped_total);
        await mock.unsubscribe(stream.response.subscription_id);
        expect(vi.getTimerCount()).toBe(0);
    });
});

describe('历史 / 工作区 / 收藏', () => {
    it('历史只记编辑器和输入框发起的调用，带参数和完整回包', async () => {
        await callAndWait(req(NC, 'get_login_info', {}, { origin: 'editor' }));
        await callAndWait(
            req(
                NC,
                'send_group_msg',
                { group_id: '100001', message: 'hi' },
                { origin: 'composer' },
            ),
        );
        await callAndWait(req(NC, 'get_group_list', {}, { origin: 'picker' }));
        await callAndWait(req(NC, 'get_status', {}, { origin: 'other' }));
        await callAndWait(req(NC, 'get_group_info', { group_id: '999' }, { origin: 'editor' }));

        const page = mock.history({
            action: null,
            bot_id: null,
            ok: null,
            text: null,
            limit: 20,
            offset: 0,
        });
        await vi.advanceTimersByTimeAsync(400);
        const { entries, total } = await page;
        expect(total).toBe(3);
        // 新的在前
        expect(entries.map((e) => e.action)).toEqual([
            'get_group_info',
            'send_group_msg',
            'get_login_info',
        ]);
        expect(entries[0]).toMatchObject({
            ok: false,
            retcode: 1200,
            bot_name: 'NapCat 测试号',
            origin: 'editor',
            error_kind: null,
        });
        expect(entries[1].origin).toBe('composer');

        const entry = mock.historyEntry(entries[1].id);
        await vi.advanceTimersByTimeAsync(400);
        expect(await entry).toMatchObject({
            action: 'send_group_msg',
            backend: 'napcat',
            params: { group_id: '100001', message: 'hi' },
            ok: true,
            response: { status: 'ok', retcode: 0 },
            response_truncated: false,
        });
    });

    it('历史里的回包：超过 256 KiB 不存只留截断标记，被截断的调用一样；凭据类动作不存也不算截断', async () => {
        // 十万字的消息发出去再 get_msg：回包近 600 KiB，没到 5 MiB，界面拿到的是完整的，但历史里不存
        const long = '长'.repeat(100_000);
        const sent = outcomeOf(
            await callAndWait(
                req(SL, 'send_group_msg', { group_id: 100001, message: long }, { origin: 'other' }),
            ),
        );
        const messageId = (sent.data as { message_id: number }).message_id;
        const got = outcomeOf(await callAndWait(req(SL, 'get_msg', { message_id: messageId })));
        expect(got.truncated).toBe(false);
        expect(got.size_bytes).toBeGreaterThan(256 * 1024);

        const huge = outcomeOf(await callAndWait(req(NC, 'debug_huge_response')));
        expect(huge.truncated).toBe(true);
        await callAndWait(req(NC, 'nc_get_rkey'));
        await callAndWait(req(NC, 'get_login_info'));

        const page = mock.history({
            action: null,
            bot_id: null,
            ok: null,
            text: null,
            limit: 20,
            offset: 0,
        });
        await vi.advanceTimersByTimeAsync(400);
        const { entries } = await page;
        expect(entries.map((e) => e.action)).toEqual([
            'get_login_info',
            'nc_get_rkey',
            'debug_huge_response',
            'get_msg',
        ]);
        const detail = entries.map((e) => mock.historyEntry(e.id));
        await vi.advanceTimersByTimeAsync(400);
        const [login, rkey, big, msg] = await Promise.all(detail);
        expect(login).toMatchObject({ response: { status: 'ok' }, response_truncated: false });
        expect(rkey).toMatchObject({ ok: true, response: null, response_truncated: false });
        expect(big).toMatchObject({
            ok: true,
            retcode: 0,
            response: null,
            response_truncated: true,
        });
        expect(msg).toMatchObject({ ok: true, response: null, response_truncated: true });
    });

    it('历史的筛选、分页和清空；没拿到回包的调用也记，带错误种类', async () => {
        await callAndWait(req(NC, 'get_login_info'));
        await callAndWait(req(SL, 'get_login_info'));
        await callAndWait(req(SL, 'get_group_info', { group_id: '999' }));
        await callAndWait(
            req(NC, 'get_group_member_list', { group_id: '100001' }, { timeout_ms: 50 }),
            100,
        );
        await callAndWait(req(DOCKER, 'get_status'), 100);

        const query = (extra: Partial<Parameters<typeof mock.history>[0]>) =>
            mock.history({
                action: null,
                bot_id: null,
                ok: null,
                text: null,
                limit: 50,
                offset: 0,
                ...extra,
            });
        const run = async <T>(p: Promise<T>) => {
            await vi.advanceTimersByTimeAsync(400);
            return p;
        };

        expect((await run(query({}))).total).toBe(5);
        expect((await run(query({ action: 'get_login_info' }))).total).toBe(2);
        expect((await run(query({ bot_id: SL }))).total).toBe(2);
        expect((await run(query({ ok: true }))).total).toBe(2);
        const failed = await run(query({ ok: false }));
        expect(failed.entries.map((e) => e.error_kind)).toEqual([
            'bot_not_running',
            'timeout',
            null,
        ]);
        expect((await run(query({ text: '999' }))).total).toBe(1);
        expect((await run(query({ text: '小雪' }))).total).toBe(2);
        const paged = await run(query({ limit: 2, offset: 1 }));
        expect(paged.entries).toHaveLength(2);
        expect(paged.total).toBe(5);

        await run(mock.clearHistory());
        expect((await run(query({}))).total).toBe(0);
        expect(await run(mock.historyEntry('mock-hist-1'))).toBeNull();
    });

    it('工作区存了再读一样，读到的是拷贝', async () => {
        const initial = mock.workspace();
        await vi.advanceTimersByTimeAsync(400);
        const workspace = await initial;
        expect(workspace).toMatchObject({
            version: 1,
            tabs: [],
            selected_bot: null,
            layout: { right_view: 'chat' },
        });

        const edited = {
            ...workspace,
            selected_bot: NC,
            recent_actions: ['send_group_msg'],
            tabs: [
                {
                    id: 't1',
                    action: 'get_status',
                    params_text: '{}',
                    timeout_ms: null,
                    channel: null,
                },
            ],
            active_tab: 't1',
        };
        const saving = mock.saveWorkspace(edited);
        edited.recent_actions.push('mutated-after-save');
        await vi.advanceTimersByTimeAsync(400);
        await saving;
        const reloaded = mock.workspace();
        await vi.advanceTimersByTimeAsync(400);
        expect(await reloaded).toMatchObject({
            selected_bot: NC,
            recent_actions: ['send_group_msg'],
            active_tab: 't1',
        });
    });

    it('收藏有一份预置，存 / 导出 / 导入都在内存里走通，导入是并进去且 id 撞了会改名', async () => {
        const wait = async <T>(p: Promise<T>) => {
            await vi.advanceTimersByTimeAsync(400);
            return p;
        };
        const seeded = await wait(mock.collections());
        expect(seeded.version).toBe(1);
        expect(seeded.folders.length).toBe(1);
        expect(seeded.requests.length).toBe(3);

        await wait(mock.exportCollections('C:/tmp/collections.json'));
        const merged = await wait(mock.importCollections('C:/tmp/collections.json'));
        expect(merged.folders.length).toBe(2);
        expect(merged.requests.length).toBe(6);
        expect(new Set(merged.requests.map((r) => r.id)).size).toBe(6);
        // 导入进来的请求跟着改名后的文件夹走
        const importedFolder = merged.folders[1];
        expect(merged.requests.filter((r) => r.folder_id === importedFolder.id).length).toBe(2);

        await wait(mock.saveCollections({ version: 1, folders: [], requests: [] }));
        expect((await wait(mock.collections())).requests).toEqual([]);

        const missing = mock.importCollections('C:/tmp/none.json');
        const assertion = expect(missing).rejects.toBe('读不到文件：C:/tmp/none.json');
        await vi.advanceTimersByTimeAsync(400);
        await assertion;
    });

    it('存储提示默认为空', async () => {
        const pending = mock.storageNotices();
        await vi.advanceTimersByTimeAsync(400);
        expect(await pending).toEqual([]);
    });

    it('预览钩子造一条存储提示：取得到，取走就清空（和后端同一 take 语义）', async () => {
        (window as unknown as { __ncdDebugStorageNotice?: () => void }).__ncdDebugStorageNotice?.();
        const first = mock.storageNotices();
        await vi.advanceTimersByTimeAsync(400);
        expect(await first).toEqual([
            {
                file: 'history.jsonl',
                moved_to: 'history.jsonl.broken-2026-09-30',
                reason: '第 3,812 行不是合法的 JSON',
            },
        ]);
        const second = mock.storageNotices();
        await vi.advanceTimersByTimeAsync(400);
        expect(await second).toEqual([]);
    });
});

describe('参数瘦身与事件流过滤（照后端规则）', () => {
    it('历史里的参数：超长字符串换成占位、整体过大的收成摘要，动过都标 params_truncated', async () => {
        const big = 'x'.repeat(70 * 1024);
        await callAndWait(
            req(
                NC,
                'send_group_msg',
                { group_id: 100001, message: 'm', file: big },
                { origin: 'editor' },
            ),
        );
        // 几千个不大不小的字段：各自没超，加起来超过整体上限，整份收成摘要
        const wide: Record<string, unknown> = { group_id: 100001, message: 'm' };
        for (let i = 0; i < 5000; i += 1) wide[`k${i}`] = 'w'.repeat(60);
        await callAndWait(req(NC, 'send_group_msg', wide, { origin: 'editor' }));

        const page = mock.history({
            action: null,
            bot_id: null,
            ok: null,
            text: null,
            limit: 10,
            offset: 0,
        });
        await vi.advanceTimersByTimeAsync(400);
        const { entries } = await page;
        expect(entries).toHaveLength(2);
        const detail = entries.map((e) => mock.historyEntry(e.id));
        await vi.advanceTimersByTimeAsync(400);
        const [newer, older] = await Promise.all(detail);
        expect(newer?.params).toMatchObject({ _omitted: expect.stringContaining('参数共') });
        expect(newer?.params_truncated).toBe(true);
        expect(older?.params).toEqual({
            group_id: 100001,
            message: 'm',
            file: expect.stringMatching(/^<已省略 \d+ 字节>$/),
        });
        expect(older?.params_truncated).toBe(true);

        // 没瘦身的照旧不标
        await callAndWait(req(NC, 'get_login_info', {}, { origin: 'editor' }));
        const slimPage = await (async () => {
            const p = mock.history({
                action: 'get_login_info',
                bot_id: null,
                ok: null,
                text: null,
                limit: 10,
                offset: 0,
            });
            await vi.advanceTimersByTimeAsync(400);
            return p;
        })();
        const slimEntry = mock.historyEntry(slimPage.entries[0].id);
        await vi.advanceTimersByTimeAsync(400);
        expect((await slimEntry)?.params_truncated).toBe(false);
    });

    it('事件流里的调用记录同样瘦身；picker 的查询不进事件流', async () => {
        const stream = await open(NC);
        await callAndWait(
            req(
                NC,
                'send_group_msg',
                { group_id: 100001, message: 'm', file: 'y'.repeat(70 * 1024) },
                { origin: 'editor' },
            ),
        );
        await callAndWait(req(NC, 'get_group_list', {}, { origin: 'picker' }));

        const records = stream
            .events()
            .flatMap((e) => (e.body.kind === 'call' ? [e.body.record] : []));
        // 只有编辑器那次：picker 拉群列表不冒调用小标签
        expect(records).toHaveLength(1);
        expect(JSON.stringify(records[0].params)).toContain('<已省略 ');
    });
});
