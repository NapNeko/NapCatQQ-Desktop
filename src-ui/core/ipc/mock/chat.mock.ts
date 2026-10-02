// 浏览器聊天预览；独立事件流不受调试台 mock 的停止操作影响。
import { onebotDebugMock } from './onebot-debug.mock';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../generated/debug/DebugCallResponse';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';
import type { DebugSubscribeResponse } from '../generated/debug/DebugSubscribeResponse';

let seq = 0;
const subscriptions = new Map<string, { bot: string; send: (batch: DebugEventBatch) => void }>();
const friends = [{ user_id: 10021, nickname: '小林', remark: '' }, { user_id: 10022, nickname: '阿澄', remark: '' }];
const groups = [{ group_id: 20001, group_name: 'NapCat 开发交流', member_count: 128 }, { group_id: 20002, group_name: '周末出游计划', member_count: 8 }];
export const chatMock = {
    targets: () => onebotDebugMock.targets(),
    async call(request: DebugCallRequest): Promise<DebugCallResponse> {
        const params = request.params as Record<string, unknown>;
        let data: unknown = null;
        const messageId = ++seq;
        if (request.action === 'get_group_list') data = groups;
        else if (request.action === 'get_friend_list' || request.action === 'get_group_member_list') data = friends;
        else if (request.action.endsWith('_msg_history')) data = { messages: [] };
        else if (request.action.startsWith('send_')) {
            data = { message_id: messageId };
            const targets = await onebotDebugMock.targets();
            const self = targets.find(t => t.bot_id === request.bot_id)?.qq_id;
            for (const sub of subscriptions.values()) if (sub.bot === request.bot_id) sub.send({ v: 1, bot_id: request.bot_id, events: [{ seq: ++seq, at_ms: Date.now(), body: { kind: 'ob11', payload: { post_type: 'message_sent', message_type: params.group_id ? 'group' : 'private', group_id: params.group_id, target_id: params.user_id, user_id: self, message_id: messageId, time: Date.now() / 1000, message: params.message } } }] });
        }
        return { request_id: request.request_id, result: { kind: 'ok', outcome: { ok: true, status: 'ok', retcode: 0, data, message: '', wording: '', raw: {}, elapsed_ms: 15, channel: { kind: 'internal' }, size_bytes: 0, truncated: false } } };
    },
    async subscribe(bot: string, send: (batch: DebugEventBatch) => void): Promise<DebugSubscribeResponse> {
        const subscription_id = crypto.randomUUID();
        subscriptions.set(subscription_id, { bot, send });
        const now = Date.now();
        const lines = [
            [20001, 10021, '小林', '新的聊天界面已经接进来了，侧栏里就能打开。'],
            [20001, 10022, '阿澄', '这个双栏很舒服，读消息的时候也能看到会话列表。'],
            [20002, 10022, '阿澄', '周六见，路线晚点发到群里。'],
        ] as const;
        send({ v: 1, bot_id: bot, events: lines.map(([group_id, user_id, nickname, content], index) => ({
            seq: ++seq, at_ms: now, body: { kind: 'ob11' as const, payload: { post_type: 'message', message_type: 'group', group_id, user_id, message_id: -(index + 1), sender: { nickname }, time: (now - (3 - index) * 120_000) / 1000, message: [{ type: 'text', data: { text: content } }] } },
        })) });
        return { subscription_id, receiver: { bot_id: bot, state: { state: 'connected' }, source: { kind: 'internal' }, buffered: 3, dropped_total: 0, first_seq: seq - 2, viewers: 1 } };
    },
    async unsubscribe(id: string): Promise<void> { subscriptions.delete(id); },
};
