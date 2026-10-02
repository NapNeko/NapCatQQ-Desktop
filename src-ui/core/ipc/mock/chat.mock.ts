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
const baseTime = Math.floor(Date.now() / 1000) - 60;
const histories = new Map<string, Record<string, unknown>[]>();
for (const [type, peer, count] of [['group', 20001, 125], ['group', 20002, 8], ['private', 10021, 12], ['private', 10022, 4]] as const) {
    histories.set(`${type}:${peer}`, Array.from({ length: count }, (_, index) => ({
        post_type: 'message', message_type: type, ...(type === 'group' ? { group_id: peer } : {}),
        user_id: type === 'group' ? index % 2 ? 10021 : 10022 : peer,
        message_id: peer * 1000 + index + 1, message_seq: String(index + 1), time: baseTime - (count - index) * 120,
        sender: { nickname: type === 'private' && peer === 10021 || index % 2 ? '小林' : '阿澄' },
        message: index % 5 === 0 ? [{ type: 'text', data: { text: `收到，晚点一起看 · ${index + 1} ` } }, { type: 'face', data: { id: '14' } }, { type: 'face', data: { id: '277' } }] : [{ type: 'text', data: { text: index === count - 1 ? type === 'group' ? '这个双栏很舒服，读消息的时候也能看到会话列表。' : '上次讨论的事情我记下了，明天继续。' : `前面的讨论 ${index + 1}：这一处细节再看看，消息记录留着方便回来找。` } }],
    })));
}
export const chatMock = {
    targets: () => onebotDebugMock.targets(),
    async call(request: DebugCallRequest): Promise<DebugCallResponse> {
        const params = request.params as Record<string, unknown>;
        let data: unknown = null;
        const messageId = ++seq;
        if (request.action === 'get_group_list') data = groups;
        else if (request.action === 'get_friend_list' || request.action === 'get_group_member_list') data = friends;
        else if (request.action === 'get_recent_contact') data = [...histories.entries()].map(([key, rows]) => {
            const [type, peer] = key.split(':'); const last = rows[rows.length - 1];
            return { chatType: type === 'group' ? 2 : 1, peerUin: peer, peerName: type === 'group' ? groups.find(g => String(g.group_id) === peer)?.group_name : friends.find(f => String(f.user_id) === peer)?.nickname, msgTime: String(last.time), lastestMsg: last };
        });
        else if (request.action.endsWith('_msg_history')) {
            const rows = histories.get(`${params.group_id ? 'group' : 'private'}:${params.group_id ?? params.user_id}`) ?? [];
            const cursor = params.message_seq ?? params.message_id;
            const index = cursor ? rows.findIndex(row => String(row.message_id) === String(cursor)) : rows.length;
            const end = index < 0 ? rows.length : index;
            data = { messages: rows.slice(Math.max(0, end - Number(params.count || 50)), end) };
        }
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
        const rows = [...histories.values()].map(rows => rows[rows.length - 1]);
        send({ v: 1, bot_id: bot, events: rows.map(payload => ({ seq: ++seq, at_ms: Date.now(), body: { kind: 'ob11' as const, payload } })) });
        return { subscription_id, receiver: { bot_id: bot, state: { state: 'connected' }, source: { kind: 'internal' }, buffered: rows.length, dropped_total: 0, first_seq: seq - rows.length + 1, viewers: 1 } };
    },
    async unsubscribe(id: string): Promise<void> { subscriptions.delete(id); },
};
