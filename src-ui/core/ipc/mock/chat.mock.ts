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
// 浏览器专用合成提示音；不作为真实账号或服务端转码结果。
function previewVoice() {
    const samples = 1600; const bytes = new Uint8Array(44 + samples * 2); const view = new DataView(bytes.buffer);
    const label = (offset: number, value: string) => Array.from(value).forEach((char, index) => { bytes[offset + index] = char.charCodeAt(0); });
    label(0, 'RIFF'); view.setUint32(4, bytes.length - 8, true); label(8, 'WAVE'); label(12, 'fmt ');
    view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true); view.setUint32(24, 8000, true); view.setUint32(28, 16000, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true); label(36, 'data'); view.setUint32(40, samples * 2, true);
    for (let index = 0; index < samples; index++) view.setInt16(44 + index * 2, Math.sin(index * 2 * Math.PI * 660 / 8000) * 2200 * Math.sin(index * Math.PI / samples), true);
    return btoa(String.fromCharCode(...bytes));
}
const mediaPreview = histories.get('group:20001');
function previewLongImage() {
    if (typeof CanvasRenderingContext2D === 'undefined') return 'https://koishi.js.org/QFace/assets/qq_emoji/277/png/277.png';
    const canvas = document.createElement('canvas'); canvas.width = 640; canvas.height = 1800;
    const context = canvas.getContext('2d'); if (!context) return '';
    context.fillStyle = '#fffaf4'; context.fillRect(0, 0, 640, 1800);
    context.fillStyle = '#272b35'; context.font = 'bold 36px sans-serif'; context.fillText('长图预览', 44, 70);
    context.font = '24px sans-serif'; context.fillStyle = '#657080'; context.fillText('合成测试图片 · 缩放后可拖动查看', 44, 112);
    for (let index = 0; index < 14; index++) {
        const top = 160 + index * 110; context.fillStyle = index % 2 ? '#edf3f6' : '#f6ebed'; context.fillRect(32, top, 576, 90);
        context.fillStyle = '#364152'; context.font = '26px sans-serif'; context.fillText(String(index + 1).padStart(2, '0') + '  这一行用来验证长图文字可读性', 48, top + 38);
        context.font = '20px sans-serif'; context.fillText('适应窗口、原图、缩放与拖动', 98, top + 68);
    }
    context.fillStyle = '#657080'; context.fillText('图片底部', 44, 1750); return canvas.toDataURL('image/png');
}
if (mediaPreview) {
    mediaPreview[mediaPreview.length - 3].message = [{ type: 'text', data: { text: '媒体预览：图片、视频与合并转发' } }, { type: 'image', data: { file: 'preview-long-image', url: previewLongImage() } }, { type: 'video', data: { file: 'preview-video' } }, { type: 'forward', data: { id: 'preview-forward' } }];
    mediaPreview[mediaPreview.length - 2].message = [{ type: 'text', data: { text: '媒体预览：合成提示音' } }, { type: 'record', data: { file: 'preview-tone' } }];
    mediaPreview[mediaPreview.length - 1].message = [{ type: 'text', data: { text: '媒体预览：语音失败与重试' } }, { type: 'record', data: { file: 'preview-error' } }];
}
export const chatMock = {
    targets: () => onebotDebugMock.targets(),
    async call(request: DebugCallRequest): Promise<DebugCallResponse> {
        const params = request.params as Record<string, unknown>;
        let data: unknown = null;
        const messageId = ++seq;
        if (request.action === 'get_group_list') data = groups;
        else if (request.action === 'get_friend_list') data = friends;
        else if (request.action === 'get_group_member_list') data = friends.map((friend, index) => ({ ...friend, group_id: params.group_id, card: index ? '阿澄' : '小林', role: index ? 'admin' : 'owner', sex: index ? 'female' : 'male', age: 24 + index, level: '12', join_time: baseTime - 86400 * 200, last_sent_time: baseTime }));
        else if (request.action === 'get_group_info') data = { ...(groups.find(group => String(group.group_id) === String(params.group_id)) ?? { group_id: params.group_id, group_name: '预览群', member_count: 2 }), max_member_count: 500, group_create_time: baseTime - 86400 * 900, group_remark: '一起讨论与记录', group_level: 3 };
        else if (request.action === 'get_stranger_info') data = { ...(friends.find(friend => String(friend.user_id) === String(params.user_id)) ?? { user_id: params.user_id, nickname: '预览好友' }), sex: 'female', age: 25, qqLevel: 36, long_nick: '认真生活，也认真记录。', country: '中国', province: '浙江', city: '杭州' };
        else if (request.action === 'fetch_custom_face') data = ['https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png', 'https://koishi.js.org/QFace/assets/qq_emoji/277/png/277.png'];
        else if (request.action === 'get_forward_msg') data = { messages: String(params.id ?? params.message_id) === 'preview-nested' ? [{ sender: { user_id: 10022, nickname: '阿澄' }, time: baseTime, message: [{ type: 'text', data: { text: '这是一条嵌套转发中的消息。' } }, { type: 'face', data: { id: '14' } }] }] : [
            { sender: { user_id: 10021, nickname: '小林' }, time: baseTime - 120, message: [{ type: 'text', data: { text: '这几条消息放在一起，回看更方便。' } }] },
            { sender: { user_id: 10022, nickname: '阿澄' }, time: baseTime - 60, message: [{ type: 'forward', data: { id: 'preview-nested' } }] },
        ] };
        else if (request.action === 'fetch_ptt_text') data = { text: '这是一段预览语音的转写内容。' };
        else if (request.action === 'get_image' && params.file === 'preview-long-image') data = { url: previewLongImage() };
        else if (request.action === 'get_file' && params.file === 'preview-video') data = { url: 'https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4', file_name: 'preview.mp4' };
        else if (request.action === 'get_record' && params.file === 'preview-tone') data = { base64: previewVoice(), out_format: 'wav' };
        else if (request.action === 'get_record') return { request_id: request.request_id, result: { kind: 'ok', outcome: { ok: false, status: 'failed', retcode: 1404, data: null, message: '', wording: '预览语音转码失败，可点击重试', raw: {}, elapsed_ms: 1, channel: { kind: 'internal' }, size_bytes: 0, truncated: false } } };
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
