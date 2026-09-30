import { describe, expect, it } from 'vitest';
import type { DebugCallRecord } from '../../ipc/generated/debug/DebugCallRecord';
import type { DebugEvent } from '../../ipc/generated/debug/DebugEvent';
import type { DebugEventBody } from '../../ipc/generated/debug/DebugEventBody';
import {
    emptyChat,
    markSessionRead,
    MAX_CHAT_ITEMS,
    reduceEvents,
    withCallWording,
    type ChatItem,
    type ChatState,
} from './chat';

const SELF = 2854196310;
const T0 = 1_700_000_000_000;

const ev = (seq: number, body: DebugEventBody, at = T0 + seq * 1000): DebugEvent => ({ seq, at_ms: at, body });
const ob = (payload: Record<string, unknown>): DebugEventBody => ({ kind: 'ob11', payload });
const obEv = (seq: number, payload: Record<string, unknown>) => ev(seq, ob(payload));

const call = (over: Partial<DebugCallRecord> = {}): DebugCallRecord => ({
    request_id: 'req-1',
    origin: 'composer',
    action: 'send_group_msg',
    params: {},
    ok: true,
    retcode: 0,
    elapsed_ms: 120,
    message_id: null,
    error: null,
    channel: { kind: 'internal' },
    ...over,
});
const callEv = (seq: number, over: Partial<DebugCallRecord> = {}) => ev(seq, { kind: 'call', record: call(over) });

const ALL = { activeSession: 'all' } as const;

/** NapCat 风格的群消息（带 group_name 和很大的 raw） */
const ncGroupMessage = (over: Record<string, unknown> = {}): Record<string, unknown> => ({
    self_id: SELF,
    user_id: 10001,
    time: 1_700_000_000,
    message_id: 7001,
    message_seq: 7001,
    real_id: 7001,
    real_seq: '7001',
    message_type: 'group',
    sender: { user_id: 10001, nickname: '小明', card: '', role: 'member' },
    raw_message: 'hello',
    font: 14,
    sub_type: 'normal',
    message: [{ type: 'text', data: { text: 'hello' } }],
    message_format: 'array',
    post_type: 'message',
    group_id: 100001,
    group_name: '测试群 1',
    raw: { msgId: '7001', elements: [] },
    ...over,
});

/** SnowLuma 风格：没有 group_name / raw */
const slGroupMessage = (over: Record<string, unknown> = {}): Record<string, unknown> => {
    const { group_name: _g, raw: _r, real_seq: _s, message_format: _f, ...rest } = ncGroupMessage();
    return { ...rest, font: 0, ...over };
};

const ncPrivateMessage = (over: Record<string, unknown> = {}): Record<string, unknown> => ({
    self_id: SELF,
    user_id: 10002,
    time: 1_700_000_001,
    message_id: 7002,
    message_type: 'private',
    sender: { user_id: 10002, nickname: 'Alice', card: '' },
    raw_message: 'hi',
    sub_type: 'friend',
    message: [{ type: 'text', data: { text: 'hi' } }],
    post_type: 'message',
    target_id: SELF,
    ...over,
});

const groupSent = (messageId: number, groupId = 100001, over: Record<string, unknown> = {}): Record<string, unknown> => ({
    self_id: SELF,
    user_id: SELF,
    time: 1_700_000_002,
    message_id: messageId,
    message_type: 'group',
    sender: { user_id: SELF, nickname: '小雪', card: '', role: 'admin' },
    raw_message: 'sent',
    sub_type: 'normal',
    message: [{ type: 'text', data: { text: 'sent' } }],
    post_type: 'message_sent',
    group_id: groupId,
    ...over,
});

const messages = (s: ChatState) => s.items.filter((i): i is Extract<ChatItem, { kind: 'message' }> => i.kind === 'message');

describe('reduceEvents · 消息', () => {
    it('别人发的群消息：入站、会话取群名、发送者名取群名片 > 昵称 > QQ 号', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage())], ALL);
        expect(s.items).toHaveLength(1);
        expect(s.items[0]).toMatchObject({
            kind: 'message',
            key: 'e1',
            seq: 1,
            at: T0 + 1000,
            session: 'group:100001',
            direction: 'in',
            senderId: 10001,
            senderName: '小明',
            senderRole: 'member',
            messageId: 7001,
            segments: [{ type: 'text', data: { text: 'hello' } }],
        });
        expect(s.sessions['group:100001']).toMatchObject({ type: 'group', id: 100001, name: '测试群 1', lastAt: T0 + 1000 });
        expect(s.lastSeq).toBe(1);

        const carded = reduceEvents(
            emptyChat(SELF),
            [obEv(1, ncGroupMessage({ sender: { user_id: 10001, nickname: '小明', card: '前端小能手' } }))],
            ALL,
        );
        expect(messages(carded)[0]?.senderName).toBe('前端小能手');

        const bare = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage({ sender: { user_id: 10001 } }))], ALL);
        expect(messages(bare)[0]?.senderName).toBe('10001');
    });

    it('原始载荷原样挂在 raw 上', () => {
        const payload = ncGroupMessage();
        const s = reduceEvents(emptyChat(SELF), [obEv(1, payload)], ALL);
        expect((s.items[0] as { raw: unknown }).raw).toBe(payload);
    });

    it('SnowLuma 的群消息没有 group_name，会话名退回「群 <id>」', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, slGroupMessage())], ALL);
        expect(s.sessions['group:100001']?.name).toBe('群 100001');
    });

    it('后来的消息带了群名，会话名更新；没带群名不会把已有的名字冲掉', () => {
        let s = reduceEvents(emptyChat(SELF), [obEv(1, slGroupMessage())], ALL);
        s = reduceEvents(s, [obEv(2, ncGroupMessage({ message_id: 7003 }))], ALL);
        expect(s.sessions['group:100001']?.name).toBe('测试群 1');
        s = reduceEvents(s, [obEv(3, slGroupMessage({ message_id: 7004 }))], ALL);
        expect(s.sessions['group:100001']?.name).toBe('测试群 1');
    });

    it('私聊：会话按对方，会话名用对方昵称', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncPrivateMessage())], ALL);
        expect(messages(s)[0]).toMatchObject({ session: 'private:10002', direction: 'in', senderName: 'Alice' });
        expect(s.sessions['private:10002']).toMatchObject({ type: 'private', id: 10002, name: 'Alice' });
    });

    it('message_sent 是出站：群按 group_id，私聊按 target_id', () => {
        const priv = groupSent(9001, 0, { message_type: 'private', user_id: SELF, target_id: 10002 });
        delete priv.group_id;
        const s = reduceEvents(emptyChat(SELF), [obEv(1, groupSent(9000)), obEv(2, priv)], ALL);
        const [a, b] = messages(s);
        expect(a).toMatchObject({ direction: 'out', session: 'group:100001', senderId: SELF, senderName: '小雪' });
        expect(b).toMatchObject({ direction: 'out', session: 'private:10002' });
        // 私聊会话没有拿到过对方昵称：用号码顶着
        expect(s.sessions['private:10002']?.name).toBe('10002');
    });

    it('user_id 等于 selfId 的 message 事件也算出站', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage({ user_id: SELF }))], ALL);
        expect(messages(s)[0]?.direction).toBe('out');
    });

    it('没给 selfId 时从事件的 self_id 学到', () => {
        let s = reduceEvents(emptyChat(), [obEv(1, ncGroupMessage())], ALL);
        expect(s.selfId).toBe(SELF);
        s = reduceEvents(s, [obEv(2, ncGroupMessage({ user_id: SELF, message_id: 7005 }))], ALL);
        expect(messages(s)[1]?.direction).toBe('out');
    });

    it('message 缺失时用 raw_message 的 CQ 码；message 是字符串时也解析', () => {
        const a = reduceEvents(
            emptyChat(SELF),
            [obEv(1, (() => { const p = ncGroupMessage({ raw_message: '看[CQ:face,id=1]' }); delete p.message; return p; })())],
            ALL,
        );
        expect(messages(a)[0]?.segments).toEqual([
            { type: 'text', data: { text: '看' } },
            { type: 'face', data: { id: '1' } },
        ]);
        const b = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage({ message: '[CQ:image,file=a.jpg]' }))], ALL);
        expect(messages(b)[0]?.segments).toEqual([{ type: 'image', data: { file: 'a.jpg' } }]);
    });
});

describe('reduceEvents · 未读', () => {
    it('别人发来、不在当前会话的消息未读 +1', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [obEv(1, ncGroupMessage()), obEv(2, ncGroupMessage({ message_id: 7010 })), obEv(3, ncPrivateMessage())],
            { activeSession: 'group:999' },
        );
        expect(s.sessions['group:100001']?.unread).toBe(2);
        expect(s.sessions['private:10002']?.unread).toBe(1);
    });

    it('正在看的会话不计未读', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage())], { activeSession: 'group:100001' });
        expect(s.sessions['group:100001']?.unread).toBe(0);
    });

    it('正在看「全部」时所有消息都在眼前，不计未读', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage())], ALL);
        expect(s.sessions['group:100001']?.unread).toBe(0);
    });

    it('自己发的消息、通知、调用都不计未读', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                obEv(1, groupSent(9000)),
                obEv(2, { post_type: 'notice', notice_type: 'group_increase', group_id: 100001, user_id: 5 }),
                callEv(3, { action: 'send_group_msg', params: { group_id: '100001', message: 'x' }, message_id: 9100 }),
            ],
            { activeSession: 'group:999' },
        );
        expect(s.sessions['group:100001']?.unread).toBe(0);
    });

    it('markSessionRead 清零，没有变化时返回同一个 state', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage())], { activeSession: 'private:1' });
        expect(s.sessions['group:100001']?.unread).toBe(1);
        const read = markSessionRead(s, 'group:100001');
        expect(read.sessions['group:100001']?.unread).toBe(0);
        expect(read.items).toBe(s.items);
        expect(s.sessions['group:100001']?.unread).toBe(1); // 旧 state 没被改
        expect(markSessionRead(read, 'group:100001')).toBe(read);
        expect(markSessionRead(read, 'group:404')).toBe(read);
    });
});

describe('reduceEvents · 调用与消息合并', () => {
    const sendParams = { group_id: '100001', message: [{ type: 'text', data: { text: 'sent' } }] };

    it('调用先到、message_sent 后到：合并成一个气泡，保留调用状态和最初的位置', () => {
        let s = reduceEvents(
            emptyChat(SELF),
            [callEv(1, { params: sendParams, message_id: 9000, elapsed_ms: 88 }), obEv(2, ncGroupMessage({ message_id: 7777 }))],
            ALL,
        );
        s = reduceEvents(s, [obEv(3, groupSent(9000))], ALL);
        const bubbles = messages(s);
        expect(bubbles).toHaveLength(2);
        const merged = bubbles[0]!;
        expect(merged).toMatchObject({
            key: 'e1',
            seq: 1,
            direction: 'out',
            session: 'group:100001',
            messageId: 9000,
            senderName: '小雪',
            senderId: SELF,
            call: { action: 'send_group_msg', requestId: 'req-1', ok: true, retcode: 0, elapsedMs: 88, channel: { kind: 'internal' } },
        });
        // 内容以事件为准
        expect((merged.raw as { post_type: string }).post_type).toBe('message_sent');
        expect(s.outByMessageId[9000]).toBe(0);
        expect(s.items).toHaveLength(2);
    });

    it('message_sent 先到、调用后到：同样合并，气泡位置是事件的位置', () => {
        let s = reduceEvents(emptyChat(SELF), [obEv(1, groupSent(9000)), obEv(2, ncGroupMessage({ message_id: 7777 }))], ALL);
        s = reduceEvents(s, [callEv(3, { params: sendParams, message_id: 9000, elapsed_ms: 42 })], ALL);
        expect(s.items).toHaveLength(2);
        expect(messages(s)[0]).toMatchObject({
            key: 'e1',
            seq: 1,
            messageId: 9000,
            senderName: '小雪',
            call: { ok: true, elapsedMs: 42 },
        });
    });

    it('同一批里两个顺序都能合并', () => {
        const a = reduceEvents(emptyChat(SELF), [callEv(1, { params: sendParams, message_id: 9000 }), obEv(2, groupSent(9000))], ALL);
        const b = reduceEvents(emptyChat(SELF), [obEv(1, groupSent(9000)), callEv(2, { params: sendParams, message_id: 9000 })], ALL);
        for (const s of [a, b]) {
            expect(s.items).toHaveLength(1);
            expect(messages(s)[0]).toMatchObject({ messageId: 9000, direction: 'out', call: { ok: true } });
        }
    });

    it('SnowLuma 的 message_id 可能是负数：两种到达顺序都能经 outByMessageId 合并', () => {
        const neg = -1_234_567_890;
        const callFirst = reduceEvents(
            emptyChat(SELF),
            [callEv(1, { params: sendParams, message_id: neg, elapsed_ms: 31 }), obEv(2, groupSent(neg))],
            ALL,
        );
        const eventFirst = reduceEvents(
            emptyChat(SELF),
            [obEv(1, groupSent(neg)), callEv(2, { params: sendParams, message_id: neg, elapsed_ms: 31 })],
            ALL,
        );
        for (const s of [callFirst, eventFirst]) {
            expect(s.items).toHaveLength(1);
            expect(s.outByMessageId[neg]).toBe(0);
            expect(messages(s)[0]).toMatchObject({ messageId: neg, direction: 'out', call: { ok: true, elapsedMs: 31 } });
        }
        // 字符串形式的负数 id（NapCat 风格的事件）也认成同一个 id
        const viaString = reduceEvents(callFirst, [obEv(3, groupSent(neg, 100001, { message_id: String(neg) }))], ALL);
        expect(viaString.items).toHaveLength(1);
    });

    it('负数 message_id 的自发气泡在 trim 之后索引仍然对得上', () => {
        const neg = -42;
        const events: DebugEvent[] = [obEv(1, groupSent(-7)), obEv(2, groupSent(neg))];
        for (let i = 0; i < MAX_CHAT_ITEMS - 2; i += 1) events.push(obEv(i + 3, ncGroupMessage({ message_id: 10000 + i })));
        let s = reduceEvents(emptyChat(SELF), events, ALL);
        expect(s.outByMessageId[-7]).toBe(0);
        // 再来一条：最早的 -7 被丢，-42 前移一位
        s = reduceEvents(s, [obEv(events.length + 1, ncGroupMessage({ message_id: 99999 }))], ALL);
        expect(s.outByMessageId[-7]).toBeUndefined();
        expect(s.outByMessageId[neg]).toBe(0);
        expect((s.items[0] as { messageId?: number }).messageId).toBe(neg);
        const merged = reduceEvents(s, [callEv(events.length + 2, { params: sendParams, message_id: neg })], ALL);
        expect(merged.items.filter((i) => i.kind === 'message' && i.messageId === neg)).toHaveLength(1);
        expect(messages(merged)[0]).toMatchObject({ messageId: neg, call: { ok: true } });
    });

    it('只有调用没有事件时，气泡内容来自 params.message，发送者是自己', () => {
        const s = reduceEvents(emptyChat(SELF), [callEv(1, { params: { group_id: 100001, message: 'hi[CQ:face,id=1]' }, message_id: 9000 })], ALL);
        expect(messages(s)[0]).toMatchObject({
            direction: 'out',
            session: 'group:100001',
            senderId: SELF,
            senderName: '我',
            messageId: 9000,
            segments: [
                { type: 'text', data: { text: 'hi' } },
                { type: 'face', data: { id: '1' } },
            ],
        });
        expect(s.sessions['group:100001']).toBeDefined();
    });

    it('各种发送动作认出目标会话', () => {
        const at = (action: string, params: Record<string, unknown>, id: number) =>
            reduceEvents(emptyChat(SELF), [callEv(1, { action, params: { message: 'x', ...params }, message_id: id })], ALL);
        expect(messages(at('send_private_msg', { user_id: '10002' }, 1))[0]?.session).toBe('private:10002');
        expect(messages(at('send_msg', { message_type: 'group', group_id: 5 }, 2))[0]?.session).toBe('group:5');
        expect(messages(at('send_msg', { message_type: 'private', user_id: 6 }, 3))[0]?.session).toBe('private:6');
        expect(messages(at('send_msg', { user_id: 7 }, 4))[0]?.session).toBe('private:7');
        expect(messages(at('send_group_forward_msg', { group_id: 8 }, 5))[0]?.session).toBe('group:8');
    });

    it('调用状态记着真正的动作：send_msg / 转发和 message_sent 合并之后也还在', () => {
        const params = { message_type: 'group', group_id: 100001, message: 'x' };
        const s = reduceEvents(
            emptyChat(SELF),
            [callEv(1, { action: 'send_msg', params, message_id: 9000, request_id: 'req-9' }), obEv(2, groupSent(9000))],
            ALL,
        );
        expect(messages(s)).toHaveLength(1);
        expect(messages(s)[0]?.call).toMatchObject({ action: 'send_msg', requestId: 'req-9' });
        const other = reduceEvents(emptyChat(SELF), [callEv(1, { action: 'get_status', request_id: null, origin: 'other' })], ALL);
        expect((other.items[0] as { call: object }).call).toMatchObject({ action: 'get_status', requestId: null });
    });

    it('转发动作的参数是 messages 节点数组，气泡里放一个 forward 段', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [callEv(1, { action: 'send_group_forward_msg', params: { group_id: 8, messages: [{ type: 'node', data: {} }] }, message_id: 5 })],
            ALL,
        );
        expect(messages(s)[0]?.segments).toEqual([{ type: 'forward', data: { messages: [{ type: 'node', data: {} }] } }]);
    });

    it('发送失败：出站气泡带失败状态，没有 messageId，不进 outByMessageId', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [callEv(1, { params: sendParams, ok: false, retcode: 1200, message_id: null, error: 'Timeout: send msg', elapsed_ms: 60000 })],
            ALL,
        );
        const [bubble] = messages(s);
        expect(bubble).toMatchObject({
            direction: 'out',
            session: 'group:100001',
            call: { ok: false, retcode: 1200, error: 'Timeout: send msg', elapsedMs: 60000 },
        });
        expect(bubble).not.toHaveProperty('messageId');
        expect(s.outByMessageId).toEqual({});
    });

    it('发送成功但没有 message_id，或认不出目标会话：只能当普通调用条目', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                callEv(1, { params: sendParams, ok: true, message_id: null }),
                callEv(2, { params: { message: 'x' }, ok: true, message_id: 5 }),
            ],
            ALL,
        );
        expect(s.items.map((i) => i.kind)).toEqual(['call', 'call']);
    });

    it('发送类动作的调用还没有结果（ok 为 null）也只是普通调用条目', () => {
        const s = reduceEvents(emptyChat(SELF), [callEv(1, { params: sendParams, ok: null })], ALL);
        expect(s.items[0]?.kind).toBe('call');
    });

    it('其它调用成为 call 条目，带来源、状态和参数摘要', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                callEv(1, {
                    action: 'get_group_member_list',
                    origin: 'picker',
                    params: { group_id: '100001', no_cache: true, note: 'x'.repeat(100), list: [1, 2, 3], obj: { a: 1 } },
                    elapsed_ms: 900,
                    channel: { kind: 'http', name: 'http-default' },
                }),
            ],
            ALL,
        );
        expect(s.items[0]).toMatchObject({
            kind: 'call',
            action: 'get_group_member_list',
            origin: 'picker',
            call: { ok: true, retcode: 0, elapsedMs: 900, channel: { kind: 'http', name: 'http-default' } },
        });
        const summary = (s.items[0] as { summary: string }).summary;
        expect(summary).toContain('group_id=100001');
        expect(summary).toContain('no_cache=true');
        expect(summary).toContain('list=[3 项]');
        expect(summary).toContain('obj={…}');
        expect(summary).not.toContain('x'.repeat(50));
    });

    it('别人发起的调用（无 request_id）、ok 为 null 也能进', () => {
        const s = reduceEvents(emptyChat(SELF), [callEv(1, { action: 'get_status', request_id: null, origin: 'other', ok: null, retcode: null })], ALL);
        expect(s.items[0]).toMatchObject({ kind: 'call', origin: 'other', summary: '' });
        expect((s.items[0] as { call: object }).call).not.toHaveProperty('ok');
    });
});

describe('reduceEvents · 通知 / 请求 / 元事件 / 状态', () => {
    const notice = (extra: Record<string, unknown>) =>
        reduceEvents(emptyChat(SELF), [obEv(1, { post_type: 'notice', self_id: SELF, time: 1, ...extra })], ALL).items[0] as Extract<
            ChatItem,
            { kind: 'notice' }
        >;

    it('入群 / 被邀请入群 / 退群 / 被踢', () => {
        expect(notice({ notice_type: 'group_increase', sub_type: 'approve', group_id: 5, user_id: 10, operator_id: 1 })).toMatchObject({
            kind: 'notice',
            text: '10 加入了群',
            session: 'group:5',
        });
        expect(notice({ notice_type: 'group_increase', sub_type: 'invite', group_id: 5, user_id: 10, operator_id: 1 }).text).toBe(
            '1 邀请 10 加入了群',
        );
        expect(notice({ notice_type: 'group_decrease', sub_type: 'leave', group_id: 5, user_id: 10, operator_id: 10 }).text).toBe('10 离开了群');
        expect(notice({ notice_type: 'group_decrease', sub_type: 'kick', group_id: 5, user_id: 10, operator_id: 1 }).text).toBe('1 把 10 移出了群');
        expect(notice({ notice_type: 'group_decrease', sub_type: 'kick_me', group_id: 5, user_id: SELF, operator_id: 1 }).text).toBe(
            `1 把 ${SELF} 移出了群`,
        );
    });

    it('禁言 / 解除 / 全员禁言', () => {
        expect(notice({ notice_type: 'group_ban', sub_type: 'ban', group_id: 5, user_id: 10, operator_id: 1, duration: 600 }).text).toBe('1 禁言了 10 600s');
        expect(notice({ notice_type: 'group_ban', sub_type: 'lift_ban', group_id: 5, user_id: 10, operator_id: 1 }).text).toBe('1 解除了 10 的禁言');
        expect(notice({ notice_type: 'group_ban', sub_type: 'ban', group_id: 5, user_id: 0, operator_id: 1, duration: -1 }).text).toBe('1 开启了全员禁言');
        expect(notice({ notice_type: 'group_ban', sub_type: 'lift_ban', group_id: 5, user_id: 0, operator_id: 1 }).text).toBe('1 关闭了全员禁言');
    });

    it('撤回：自己撤回写一个人，管理员撤回别人的写清楚是谁的', () => {
        expect(notice({ notice_type: 'group_recall', group_id: 5, user_id: 10, operator_id: 10, message_id: 3 }).text).toBe('10 撤回了一条消息');
        expect(notice({ notice_type: 'group_recall', group_id: 5, user_id: 10, operator_id: 1, message_id: 3 }).text).toBe(
            '1 撤回了 10 的一条消息',
        );
        expect(notice({ notice_type: 'group_recall', group_id: 5, user_id: 10, message_id: 3 }).text).toBe('10 撤回了一条消息');
        expect(notice({ notice_type: 'friend_recall', user_id: 10, message_id: 3 })).toMatchObject({
            text: '10 撤回了一条消息',
            session: 'private:10',
        });
    });

    it('戳一戳：群里按 user_id / target_id，私聊按 sender_id', () => {
        expect(notice({ notice_type: 'notify', sub_type: 'poke', group_id: 5, user_id: 10, target_id: SELF }).text).toBe(`10 戳了戳 ${SELF}`);
        expect(notice({ notice_type: 'notify', sub_type: 'poke', user_id: 10, sender_id: 10, target_id: SELF })).toMatchObject({
            text: `10 戳了戳 ${SELF}`,
            session: 'private:10',
        });
    });

    it('管理员 / 上传 / 加好友 / 精华 / 群名片', () => {
        expect(notice({ notice_type: 'group_admin', sub_type: 'set', group_id: 5, user_id: 10 }).text).toBe('10 成为了管理员');
        expect(notice({ notice_type: 'group_admin', sub_type: 'unset', group_id: 5, user_id: 10 }).text).toBe('10 被取消了管理员');
        expect(notice({ notice_type: 'group_upload', group_id: 5, user_id: 10, file: { id: 'f', name: 'a.txt', size: 1 } }).text).toBe(
            '10 上传了文件 a.txt',
        );
        expect(notice({ notice_type: 'friend_add', user_id: 10 })).toMatchObject({ text: '10 成为了你的好友', session: 'private:10' });
        expect(notice({ notice_type: 'essence', sub_type: 'add', group_id: 5, operator_id: 1, sender_id: 10, message_id: 3 }).text).toBe(
            '1 把 10 的一条消息设为精华',
        );
        expect(notice({ notice_type: 'essence', sub_type: 'add', group_id: 5, operator_id: 1, message_id: 3 }).text).toBe('1 把一条消息设为精华');
        expect(notice({ notice_type: 'essence', sub_type: 'delete', group_id: 5, operator_id: 1 }).text).toBe('1 取消了一条精华消息');
        expect(notice({ notice_type: 'group_card', group_id: 5, user_id: 10, card_new: '新名片', card_old: '' }).text).toBe(
            '10 的群名片改为「新名片」',
        );
    });

    it('不认识的通知：类型 / 子类型', () => {
        expect(notice({ notice_type: 'group_msg_emoji_like', group_id: 5 }).text).toBe('group_msg_emoji_like');
        expect(notice({ notice_type: 'notify', sub_type: 'lucky_king', group_id: 5 }).text).toBe('notify/lucky_king');
        expect(notice({ notice_type: 'foo', sub_type: 'bar' }).text).toBe('foo/bar');
    });

    it('缺少用户 id 时不出现 undefined', () => {
        expect(notice({ notice_type: 'group_increase', group_id: 5 }).text).toBe('某人 加入了群');
    });

    it('好友请求 / 加群请求', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                obEv(1, { post_type: 'request', request_type: 'friend', user_id: 10, comment: '加一下', flag: 'f1', self_id: SELF }),
                obEv(2, { post_type: 'request', request_type: 'group', sub_type: 'add', group_id: 5, user_id: 11, comment: '进群', flag: 'f2' }),
            ],
            ALL,
        );
        expect(s.items[0]).toMatchObject({ kind: 'request', requestType: 'friend', userId: 10, comment: '加一下', flag: 'f1' });
        expect(s.items[0]).not.toHaveProperty('groupId');
        expect(s.items[1]).toMatchObject({ kind: 'request', requestType: 'group', userId: 11, groupId: 5, flag: 'f2' });
    });

    it('心跳与生命周期都是 meta，只有心跳标 heartbeat', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                obEv(1, { post_type: 'meta_event', meta_event_type: 'lifecycle', sub_type: 'connect' }),
                obEv(2, { post_type: 'meta_event', meta_event_type: 'heartbeat', interval: 30000, status: { online: true } }),
            ],
            ALL,
        );
        expect(s.items[0]).toMatchObject({ kind: 'meta', heartbeat: false, text: '生命周期：连接建立' });
        expect(s.items[1]).toMatchObject({ kind: 'meta', heartbeat: true, text: '心跳' });
    });

    it('不认识的 post_type 也留一条 meta，不悄悄吞掉', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(1, { post_type: 'weird' }), obEv(2, {})], ALL);
        expect(s.items[0]).toMatchObject({ kind: 'meta', heartbeat: false, text: '未知事件 weird' });
        expect(s.items[1]).toMatchObject({ kind: 'meta' });
    });

    it('缺口 / 丢弃 / 接收器状态变成对应条目', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                ev(1, { kind: 'gap', from_ms: T0, to_ms: T0 + 5000 }),
                ev(2, { kind: 'dropped', count: 37 }),
                ev(3, { kind: 'receiver', state: { state: 'reconnecting', attempt: 2, retry_in_ms: 2000 }, source: { kind: 'internal' } }),
            ],
            ALL,
        );
        expect(s.items).toEqual([
            { kind: 'gap', key: 'e1', seq: 1, at: T0 + 1000, fromMs: T0, toMs: T0 + 5000 },
            { kind: 'dropped', key: 'e2', seq: 2, at: T0 + 2000, count: 37 },
            { kind: 'receiver', key: 'e3', seq: 3, at: T0 + 3000, state: { state: 'reconnecting', attempt: 2, retry_in_ms: 2000 } },
        ]);
    });
});

describe('reduceEvents · 通知里的人名', () => {
    const noticeEv = (seq: number, extra: Record<string, unknown>) => obEv(seq, { post_type: 'notice', self_id: SELF, time: 1, ...extra });
    const texts = (s: ChatState) => s.items.filter((i) => i.kind === 'notice').map((i) => (i as { text: string }).text);
    /** 先让几个人在群里说过话，名字就记下了 */
    const known = (): ChatState =>
        reduceEvents(
            emptyChat(SELF),
            [
                obEv(1, ncGroupMessage({ user_id: 10, message_id: 1, sender: { user_id: 10, nickname: '小明', card: '' } })),
                obEv(2, ncGroupMessage({ user_id: 1, message_id: 2, sender: { user_id: 1, nickname: 'admin', card: '群主大人' } })),
                obEv(3, groupSent(3)),
            ],
            ALL,
        );

    it('从 message / message_sent 的 sender 记名字：群名片优先，没有就昵称', () => {
        const s = known();
        expect(s.names.get(10)).toBe('小明');
        expect(s.names.get(1)).toBe('群主大人');
        expect(s.names.get(SELF)).toBe('小雪');
    });

    it('通知用认得的名字，没见过的号照旧写号', () => {
        const s = reduceEvents(
            known(),
            [
                noticeEv(10, { notice_type: 'notify', sub_type: 'poke', group_id: 5, user_id: 10, target_id: SELF }),
                noticeEv(11, { notice_type: 'group_recall', group_id: 5, user_id: 10, operator_id: 1, message_id: 3 }),
                noticeEv(12, { notice_type: 'group_ban', sub_type: 'ban', group_id: 5, user_id: 10, operator_id: 1, duration: 60 }),
                noticeEv(13, { notice_type: 'group_ban', sub_type: 'lift_ban', group_id: 5, user_id: 10, operator_id: 1 }),
                noticeEv(14, { notice_type: 'group_decrease', sub_type: 'kick', group_id: 5, user_id: 77777, operator_id: 1 }),
                noticeEv(15, { notice_type: 'group_decrease', sub_type: 'leave', group_id: 5, user_id: 10, operator_id: 10 }),
                noticeEv(16, { notice_type: 'group_increase', sub_type: 'invite', group_id: 5, user_id: 88888, operator_id: 10 }),
                noticeEv(17, { notice_type: 'group_admin', sub_type: 'set', group_id: 5, user_id: 10 }),
                noticeEv(18, { notice_type: 'group_upload', group_id: 5, user_id: 10, file: { name: 'a.txt' } }),
                noticeEv(19, { notice_type: 'essence', sub_type: 'add', group_id: 5, operator_id: 1, sender_id: 10 }),
                noticeEv(20, { notice_type: 'friend_recall', user_id: 10, message_id: 4 }),
            ],
            ALL,
        );
        expect(texts(s)).toEqual([
            '小明 戳了戳 小雪',
            '群主大人 撤回了 小明 的一条消息',
            '群主大人 禁言了 小明 60s',
            '群主大人 解除了 小明 的禁言',
            '群主大人 把 77777 移出了群',
            '小明 离开了群',
            '小明 邀请 88888 加入了群',
            '小明 成为了管理员',
            '小明 上传了文件 a.txt',
            '群主大人 把 小明 的一条消息设为精华',
            '小明 撤回了一条消息',
        ]);
    });

    it('同一批里先说话后被戳也认得；改了群名片之后的通知用新名片', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [
                obEv(1, ncGroupMessage({ user_id: 10, message_id: 1, sender: { user_id: 10, nickname: '小明', card: '' } })),
                noticeEv(2, { notice_type: 'group_card', group_id: 5, user_id: 10, card_new: '明哥', card_old: '' }),
                noticeEv(3, { notice_type: 'notify', sub_type: 'poke', group_id: 5, user_id: 10, target_id: 20 }),
            ],
            ALL,
        );
        expect(texts(s)).toEqual(['小明 的群名片改为「明哥」', '明哥 戳了戳 20']);
    });

    it('名字表有上限：满了丢掉最早记下的一半', () => {
        let s = emptyChat(SELF);
        let seq = 0;
        for (let b = 0; b < 11; b += 1) {
            const events: DebugEvent[] = [];
            for (let i = 0; i < 1000; i += 1) {
                seq += 1;
                events.push(obEv(seq, ncGroupMessage({ user_id: 100000 + seq, message_id: seq, sender: { nickname: `u${seq}` } })));
            }
            s = reduceEvents(s, events, ALL);
        }
        // 第 11 批拷表时已有 10000 个：先丢掉最早的 5000 个，再记新的 1000 个
        expect(s.names.size).toBe(6000);
        expect(s.names.get(100001)).toBeUndefined();
        expect(s.names.get(100000 + seq)).toBe(`u${seq}`);
    });
});

describe('失败调用上游给的说明（wording）', () => {
    const failed = (seq: number, requestId: string | null = 'req-1', over: Partial<DebugCallRecord> = {}) =>
        callEv(seq, {
            request_id: requestId,
            params: { group_id: 100001, message: 'x' },
            ok: false,
            retcode: 1200,
            message_id: null,
            ...over,
        });

    it('回包先到：reduce 到那条失败的调用时来取，每条只问一次', () => {
        const asked: string[] = [];
        const s = reduceEvents(emptyChat(SELF), [failed(1), callEv(2, { request_id: 'req-2', action: 'get_status' })], {
            activeSession: 'all',
            callWording: (id) => {
                asked.push(id);
                return id === 'req-1' ? '消息内容为空' : undefined;
            },
        });
        expect(messages(s)[0]?.call).toMatchObject({ ok: false, retcode: 1200, wording: '消息内容为空', requestId: 'req-1' });
        // 成功的调用不问
        expect(asked).toEqual(['req-1']);
    });

    it('别人发起的调用（没有 request_id）不问', () => {
        const callWording = () => '不该出现';
        const s = reduceEvents(emptyChat(SELF), [failed(1, null)], { activeSession: 'all', callWording });
        expect(messages(s)[0]?.call).not.toHaveProperty('wording');
    });

    it('事件先到：withCallWording 补到已经在时间线上的那条；普通调用条目也补', () => {
        const s = reduceEvents(
            emptyChat(SELF),
            [failed(1), failed(2, 'req-2', { action: 'get_group_info', params: { group_id: 1 } }), obEv(3, ncGroupMessage())],
            ALL,
        );
        const patched = withCallWording(s, 'req-1', '消息内容为空');
        expect(patched).not.toBe(s);
        expect(messages(patched)[0]?.call?.wording).toBe('消息内容为空');
        expect(s.items[0]).not.toBe(patched.items[0]);
        expect(patched.items[1]).toBe(s.items[1]);
        expect(patched.items[2]).toBe(s.items[2]);

        const onCall = withCallWording(patched, 'req-2', '群不存在');
        expect(onCall.items[1]).toMatchObject({ kind: 'call', call: { wording: '群不存在' } });
    });

    it('找不到、成功的调用、或者已经是这句话：原样返回同一个 state', () => {
        const s = reduceEvents(emptyChat(SELF), [failed(1), callEv(2, { request_id: 'req-ok', action: 'get_status' })], ALL);
        expect(withCallWording(s, 'req-x', '什么')).toBe(s);
        expect(withCallWording(s, 'req-ok', '什么')).toBe(s);
        const once = withCallWording(s, 'req-1', '消息内容为空');
        expect(withCallWording(once, 'req-1', '消息内容为空')).toBe(once);
    });
});

describe('reduceEvents · 幂等、不可变、trim', () => {
    const batch = (): DebugEvent[] => [
        obEv(1, ncGroupMessage()),
        callEv(2, { params: { group_id: '100001', message: 'x' }, message_id: 9000 }),
        obEv(3, groupSent(9000)),
        obEv(4, { post_type: 'notice', notice_type: 'group_increase', group_id: 100001, user_id: 5 }),
    ];

    it('同一批重复喂进来：结果不变，且直接返回原 state', () => {
        const once = reduceEvents(emptyChat(SELF), batch(), { activeSession: 'private:1' });
        const twice = reduceEvents(once, batch(), { activeSession: 'private:1' });
        expect(twice).toBe(once);
        expect(twice.items).toHaveLength(3);
        expect(twice.sessions['group:100001']?.unread).toBe(1);
    });

    it('批内前半段是旧的、后半段是新的：只处理新的', () => {
        const first = reduceEvents(emptyChat(SELF), batch().slice(0, 2), ALL);
        const all = reduceEvents(first, batch(), ALL);
        const direct = reduceEvents(emptyChat(SELF), batch(), ALL);
        expect(all.items).toEqual(direct.items);
        expect(all.lastSeq).toBe(4);
    });

    it('seq 不大于 lastSeq 的乱序事件被忽略', () => {
        const s = reduceEvents(emptyChat(SELF), [obEv(5, ncGroupMessage()), obEv(3, ncGroupMessage({ message_id: 1 }))], ALL);
        expect(s.items).toHaveLength(1);
        expect(s.lastSeq).toBe(5);
    });

    it('空批次原样返回', () => {
        const s = emptyChat(SELF);
        expect(reduceEvents(s, [], ALL)).toBe(s);
    });

    it('不修改传入的 state', () => {
        const before = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage()), obEv(2, groupSent(9000))], { activeSession: 'private:1' });
        const snapshot = JSON.stringify(before);
        const itemsRef = before.items;
        const sessionsRef = before.sessions;
        const outRef = before.outByMessageId;
        const namesRef = before.names;
        const namesBefore = [...before.names];
        reduceEvents(
            before,
            [
                callEv(3, { params: { group_id: '100001', message: 'x' }, message_id: 9000 }),
                obEv(4, ncGroupMessage({ message_id: 8, user_id: 10009, sender: { user_id: 10009, nickname: '新来的' } })),
            ],
            { activeSession: 'private:1' },
        );
        expect(JSON.stringify(before)).toBe(snapshot);
        expect(before.items).toBe(itemsRef);
        expect(before.sessions).toBe(sessionsRef);
        expect(before.outByMessageId).toBe(outRef);
        expect(before.names).toBe(namesRef);
        expect([...before.names]).toEqual(namesBefore);
    });

    it('没碰到的部分沿用旧引用：只有缺口事件时会话表和自发索引不重建', () => {
        const before = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage()), obEv(2, groupSent(9000))], ALL);
        const after = reduceEvents(before, [ev(3, { kind: 'dropped', count: 1 })], ALL);
        expect(after.sessions).toBe(before.sessions);
        expect(after.outByMessageId).toBe(before.outByMessageId);
        expect(after.names).toBe(before.names);
        expect(after.items).not.toBe(before.items);
    });

    it('名字表只在有新名字或改了名时才换：熟人刷屏不拷表', () => {
        const before = reduceEvents(emptyChat(SELF), [obEv(1, ncGroupMessage())], ALL);
        const flood: DebugEvent[] = [];
        for (let i = 2; i <= 3001; i += 1) flood.push(obEv(i, ncGroupMessage({ message_id: i })));
        const after = reduceEvents(before, flood, ALL);
        expect(after.items).toHaveLength(3001);
        expect(after.names).toBe(before.names);

        const renamed = reduceEvents(after, [obEv(3002, ncGroupMessage({ message_id: 1, sender: { user_id: 10001, nickname: '小明', card: '明哥' } }))], ALL);
        expect(renamed.names).not.toBe(after.names);
        expect(renamed.names.get(10001)).toBe('明哥');
        expect(after.names.get(10001)).toBe('小明');
    });

    it('超过上限只保留最新的 MAX_CHAT_ITEMS 条', () => {
        const extra = 25;
        const events: DebugEvent[] = [];
        for (let i = 1; i <= MAX_CHAT_ITEMS + extra; i += 1) events.push(obEv(i, ncGroupMessage({ message_id: 100000 + i })));
        // 分两批喂：一批塞满，一批溢出
        let s = reduceEvents(emptyChat(SELF), events.slice(0, MAX_CHAT_ITEMS), ALL);
        expect(s.items).toHaveLength(MAX_CHAT_ITEMS);
        s = reduceEvents(s, events.slice(MAX_CHAT_ITEMS), ALL);
        expect(s.items).toHaveLength(MAX_CHAT_ITEMS);
        expect(s.items[0]?.seq).toBe(extra + 1);
        expect(s.items[s.items.length - 1]?.seq).toBe(MAX_CHAT_ITEMS + extra);
        expect(s.lastSeq).toBe(MAX_CHAT_ITEMS + extra);
    });

    it('trim 之后 outByMessageId 的下标仍然指向对的气泡，被丢掉的不再有索引', () => {
        // 前 10 条里有 3 个自发气泡（会被丢），后面还有 3 个（保留）
        const events: DebugEvent[] = [];
        let seq = 0;
        const push = (payload: Record<string, unknown>) => events.push(obEv(++seq, payload));
        for (let i = 0; i < 10; i += 1) {
            if (i % 4 === 0) push(groupSent(5000 + i));
            else push(ncGroupMessage({ message_id: 100 + i }));
        }
        for (let i = 0; i < MAX_CHAT_ITEMS - 14; i += 1) push(ncGroupMessage({ message_id: 10000 + i }));
        push(groupSent(6001));
        push(ncGroupMessage({ message_id: 200 }));
        push(groupSent(6002));
        push(groupSent(6003));
        // 到这里刚好塞满；再补 10 条，逼出 trim（丢掉最前面的 10 条）
        const lead = events.length;
        expect(lead).toBe(MAX_CHAT_ITEMS);
        for (let i = 0; i < 10; i += 1) push(ncGroupMessage({ message_id: 20000 + i }));

        let s = reduceEvents(emptyChat(SELF), events.slice(0, lead), ALL);
        expect(s.items.length).toBe(lead);
        s = reduceEvents(s, events.slice(lead), ALL);
        expect(s.items).toHaveLength(MAX_CHAT_ITEMS);

        for (const [id, index] of Object.entries(s.outByMessageId)) {
            const item = s.items[index];
            expect(item?.kind).toBe('message');
            expect((item as { messageId?: number }).messageId).toBe(Number(id));
            expect((item as { direction?: string }).direction).toBe('out');
        }
        // 保留的三个都在，被丢的三个不在
        expect(Object.keys(s.outByMessageId).map(Number).sort((a, b) => a - b)).toEqual([6001, 6002, 6003]);

        // 索引没错位：trim 之后再来一条针对保留气泡的调用，能合并上而不是新建
        const seqNext = seq + 1;
        const merged = reduceEvents(
            s,
            [callEv(seqNext, { params: { group_id: '100001', message: 'x' }, message_id: 6002, elapsed_ms: 7 })],
            ALL,
        );
        const target = merged.items.filter((i) => i.kind === 'message' && i.messageId === 6002);
        expect(target).toHaveLength(1);
        expect(target[0]).toMatchObject({ call: { ok: true, elapsedMs: 7 } });
        expect(merged.items).toHaveLength(MAX_CHAT_ITEMS);
    });

    it('被 trim 掉的气泡再来 message_sent / 调用，当新气泡处理，不会串到别的条目上', () => {
        const filler: DebugEvent[] = [obEv(1, groupSent(4000))];
        for (let i = 0; i < MAX_CHAT_ITEMS; i += 1) filler.push(obEv(i + 2, ncGroupMessage({ message_id: 50000 + i })));
        const s = reduceEvents(emptyChat(SELF), filler, ALL);
        expect(s.outByMessageId[4000]).toBeUndefined();
        const next = reduceEvents(s, [callEv(filler.length + 1, { params: { group_id: '100001', message: 'late' }, message_id: 4000 })], ALL);
        const hits = next.items.filter((i) => i.kind === 'message' && i.messageId === 4000);
        expect(hits).toHaveLength(1);
        expect(next.outByMessageId[4000]).toBe(next.items.length - 1);
    });

    it('大批量事件线性处理：两万条分批喂完，条目和索引都自洽', () => {
        let s = emptyChat(SELF);
        let seq = 0;
        for (let b = 0; b < 400; b += 1) {
            const events: DebugEvent[] = [];
            for (let i = 0; i < 50; i += 1) {
                seq += 1;
                events.push(obEv(seq, seq % 10 === 0 ? groupSent(seq) : ncGroupMessage({ message_id: seq })));
            }
            s = reduceEvents(s, events, { activeSession: 'private:1' });
        }
        expect(s.items).toHaveLength(MAX_CHAT_ITEMS);
        expect(s.lastSeq).toBe(20000);
        expect(Object.keys(s.outByMessageId)).toHaveLength(MAX_CHAT_ITEMS / 10);
        for (const [id, index] of Object.entries(s.outByMessageId)) {
            expect((s.items[index] as { messageId?: number }).messageId).toBe(Number(id));
        }
        expect(s.sessions['group:100001']?.unread).toBe(18000);
    });
});
