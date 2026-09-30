import { describe, expect, it } from 'vitest';
import type { ChatCallState, ChatItem } from './chat';
import { callLine, listRowOf, needsTimeSeparator, planFill, sendActionOf, type MessageItem } from './chatFormat';
import type { FormField } from './schemaForm';

const call = (over: Partial<ChatCallState> = {}): ChatCallState => ({
    action: 'send_group_msg',
    requestId: 'req-1',
    ok: true,
    retcode: 0,
    elapsedMs: 128,
    error: null,
    channel: { kind: 'internal' },
    ...over,
});

const bubble = (over: Partial<MessageItem> = {}): MessageItem => ({
    kind: 'message',
    key: 'e1',
    seq: 1,
    at: 0,
    session: 'group:100001',
    direction: 'out',
    senderId: 10000,
    senderName: '我',
    messageId: 9000,
    segments: [{ type: 'text', data: { text: '你好' } }],
    raw: {},
    ...over,
});

const field = (name: string, kind: FormField['kind'], accepts: { string: boolean; number: boolean }): FormField => ({
    name,
    label: name,
    required: true,
    kind,
    valueType: accepts.number ? 'integer' : 'string',
    acceptsString: accepts.string,
    acceptsNumber: accepts.number,
});

describe('callLine', () => {
    it('成功：耗时和通道', () => {
        expect(callLine(call())).toEqual({ ok: true, text: '✓ 128ms · 内部通道' });
        expect(callLine(call({ elapsedMs: null, channel: null }))).toEqual({ ok: true, text: '✓' });
    });

    it('OB11 层失败：retcode 加上游给的说明；没有说明就只写 retcode', () => {
        expect(callLine(call({ ok: false, retcode: 1200, elapsedMs: 30, wording: '消息内容为空' }))).toEqual({
            ok: false,
            text: '✗ retcode 1200 · 消息内容为空',
        });
        expect(callLine(call({ ok: false, retcode: 1200 }))).toEqual({ ok: false, text: '✗ retcode 1200' });
    });

    it('没拿到回包：写没拿到的原因', () => {
        expect(callLine(call({ ok: false, retcode: null, elapsedMs: 30000, error: '等太久了，调用超时' }))).toEqual({
            ok: false,
            text: '✗ 失败 · 等太久了，调用超时',
        });
    });
});

describe('sendActionOf', () => {
    it('按调用状态里记的动作写，send_msg、转发不会被说成 send_group_msg', () => {
        expect(sendActionOf(bubble({ call: call({ action: 'send_msg' }) }))).toBe('send_msg');
        expect(sendActionOf(bubble({ call: call({ action: 'send_group_forward_msg' }) }))).toBe('send_group_forward_msg');
    });

    it('没有动作可看时才按会话类型推断', () => {
        expect(sendActionOf(bubble())).toBe('send_group_msg');
        expect(sendActionOf(bubble({ session: 'private:10002' }))).toBe('send_private_msg');
        expect(sendActionOf(bubble({ call: call({ action: '' }) }))).toBe('send_group_msg');
    });
});

describe('listRowOf', () => {
    it('消息：post_type/会话类型，摘要带发送者；发送失败标红', () => {
        const inbound = bubble({ direction: 'in', senderName: '小明', raw: { post_type: 'message' } });
        expect(listRowOf(inbound)).toEqual({ type: 'message/group', summary: '小明：你好', tone: 'message' });
        // 调用先到、还没被 message_sent 合并：raw 是调用记录，没有 post_type
        const failed = bubble({ session: 'private:10002', segments: [], call: call({ ok: false, retcode: 100 }) });
        expect(listRowOf(failed)).toEqual({ type: 'call/private', summary: '我：（空消息）', tone: 'danger' });
    });

    it('调用：动作名 + 状态行 + 参数摘要', () => {
        const item: ChatItem = {
            kind: 'call',
            key: 'e2',
            seq: 2,
            at: 0,
            action: 'get_group_info',
            origin: 'editor',
            call: call({ action: 'get_group_info', elapsedMs: 9 }),
            summary: 'group_id=100001',
            raw: {},
        };
        expect(listRowOf(item)).toEqual({ type: 'call/get_group_info', summary: '✓ 9ms · 内部通道  group_id=100001', tone: 'call' });
        const failed = { ...item, call: call({ ok: false, retcode: 1404, wording: '群不存在' }) };
        expect(listRowOf(failed)).toMatchObject({ summary: '✗ retcode 1404 · 群不存在  group_id=100001', tone: 'danger' });
    });
});

describe('planFill', () => {
    const ids = { group_id: 100001, user_id: 10001, message_id: 5001 };

    it('值的类型跟着字段走：只收字符串的给字符串，收数字的给数字', () => {
        const fields = [
            field('group_id', 'group', { string: true, number: false }),
            field('user_id', 'member', { string: false, number: true }),
        ];
        const plan = planFill('{}', fields, ids);
        expect(plan.ok).toBe(true);
        if (!plan.ok) return;
        expect(plan.filled).toEqual(['group_id', 'user_id']);
        expect(JSON.parse(plan.text)).toEqual({ group_id: '100001', user_id: 10001 });
    });

    it('两种都收时跟着文本里原来的值；说明里没有的键，文本里有才填', () => {
        const fields = [field('group_id', 'group', { string: true, number: true })];
        const plan = planFill('{ "group_id": "1", "message_id": 0 }', fields, ids);
        expect(plan.ok && JSON.parse(plan.text)).toEqual({ group_id: '100001', message_id: 5001 });
    });

    it('JSON 写坏了不填，告诉用户第几行', () => {
        expect(planFill('{\n  "group_id": ,\n}', [], ids)).toEqual({ ok: false, reason: '当前请求的 JSON 第 2 行有错，改好再填' });
    });

    it('没有能填的参数', () => {
        expect(planFill('{"no_cache": true}', [], ids)).toEqual({
            ok: false,
            reason: '当前请求里没有能填的群号 / QQ 号 / 消息 id 参数',
        });
    });
});

describe('needsTimeSeparator', () => {
    const base = new Date(2026, 8, 29, 14, 0, 0).getTime();

    it('第一条总插；隔 5 分钟以内不插，超过就插', () => {
        expect(needsTimeSeparator(undefined, base)).toBe(true);
        expect(needsTimeSeparator(base, base + 5 * 60_000)).toBe(false);
        expect(needsTimeSeparator(base, base + 5 * 60_000 + 1)).toBe(true);
    });

    it('只隔两分钟但跨过了零点也插', () => {
        const midnight = new Date(2026, 8, 30, 0, 0, 0).getTime();
        expect(needsTimeSeparator(midnight - 60_000, midnight + 60_000)).toBe(true);
        expect(needsTimeSeparator(midnight + 60_000, midnight + 120_000)).toBe(false);
    });
});
