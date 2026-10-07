// 浏览器聊天预览；独立事件流不受调试台 mock 的停止操作影响。
import { onebotDebugMock } from './onebot-debug.mock';
import { QQ_FACE_FALLBACK } from '../../domain/chat/qqFaces';
import type { DebugCallRequest } from '../generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../generated/debug/DebugCallResponse';
import type { DebugEventBatch } from '../generated/debug/DebugEventBatch';
import type { DebugSubscribeResponse } from '../generated/debug/DebugSubscribeResponse';
import type { DebugStreamProgress } from '../generated/debug/DebugStreamProgress';
import { chatGroupFilesMock } from './chat-group-files.mock';

let seq = 0;
const subscriptions = new Map<string, { bot: string; send: (batch: DebugEventBatch) => void }>();
const friends = [
    { user_id: 10021, nickname: '小林', remark: '' },
    { user_id: 10022, nickname: '阿澄', remark: '' },
];
const groups = [
    { group_id: 20001, group_name: 'NapCat 开发交流', member_count: 128 },
    { group_id: 20002, group_name: '周末出游计划', member_count: 8 },
];
const baseTime = Math.floor(Date.now() / 1000) - 60;
const histories = new Map<string, Record<string, unknown>[]>();
const forwardedMessages = new Map<string, Record<string, unknown>[]>();
const messageOwners = new Map<string, string>();
const favoriteFaces = new Map<string, Array<{ url: string; desc: string }>>();
const downloadedFaces = new Map<string, string>();
const removedMembers = new Map<string, Set<string>>();
const mutedMembers = new Map<string, number>();
function favorites(bot: string) {
    if (!favoriteFaces.has(bot))
        favoriteFaces.set(bot, [
            { url: 'https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png', desc: '开心' },
            { url: 'https://koishi.js.org/QFace/assets/qq_emoji/277/png/277.png', desc: '汪汪' },
        ]);
    return favoriteFaces.get(bot)!;
}
function failed(request: DebugCallRequest, wording: string): DebugCallResponse {
    return {
        request_id: request.request_id,
        result: {
            kind: 'ok',
            outcome: {
                ok: false,
                status: 'failed',
                retcode: 1404,
                data: null,
                message: '',
                wording,
                raw: {},
                elapsed_ms: 1,
                channel: { kind: 'internal' },
                size_bytes: 0,
                truncated: false,
            },
        },
    };
}
for (const [type, peer, count] of [
    ['group', 20001, 125],
    ['group', 20002, 8],
    ['private', 10021, 12],
    ['private', 10022, 4],
] as const) {
    histories.set(
        `${type}:${peer}`,
        Array.from({ length: count }, (_, index) => ({
            post_type: 'message',
            message_type: type,
            ...(type === 'group' ? { group_id: peer } : {}),
            user_id: type === 'group' ? (index % 2 ? 10021 : 10022) : peer,
            message_id: peer * 1000 + index + 1,
            message_seq: String(index + 1),
            time: baseTime - (count - index) * 120,
            sender: {
                nickname: (type === 'private' && peer === 10021) || index % 2 ? '小林' : '阿澄',
            },
            message:
                index % 5 === 0
                    ? [
                          { type: 'text', data: { text: `收到，晚点一起看 · ${index + 1} ` } },
                          { type: 'face', data: { id: '14' } },
                          { type: 'face', data: { id: '277' } },
                      ]
                    : [
                          {
                              type: 'text',
                              data: {
                                  text:
                                      index === count - 1
                                          ? type === 'group'
                                              ? '这个双栏很舒服，读消息的时候也能看到会话列表。'
                                              : '上次讨论的事情我记下了，明天继续。'
                                          : `前面的讨论 ${index + 1}：这一处细节再看看，消息记录留着方便回来找。`,
                              },
                          },
                      ],
        })),
    );
}
// 浏览器专用合成提示音；不作为真实账号或服务端转码结果。
function previewVoice() {
    const samples = 1600;
    const bytes = new Uint8Array(44 + samples * 2);
    const view = new DataView(bytes.buffer);
    const label = (offset: number, value: string) =>
        Array.from(value).forEach((char, index) => {
            bytes[offset + index] = char.charCodeAt(0);
        });
    label(0, 'RIFF');
    view.setUint32(4, bytes.length - 8, true);
    label(8, 'WAVE');
    label(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, 8000, true);
    view.setUint32(28, 16000, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    label(36, 'data');
    view.setUint32(40, samples * 2, true);
    for (let index = 0; index < samples; index++)
        view.setInt16(
            44 + index * 2,
            Math.sin((index * 2 * Math.PI * 660) / 8000) *
                2200 *
                Math.sin((index * Math.PI) / samples),
            true,
        );
    return btoa(String.fromCharCode(...bytes));
}
const mediaPreview = histories.get('group:20001');
function previewLongImage() {
    if (typeof CanvasRenderingContext2D === 'undefined')
        return 'https://koishi.js.org/QFace/assets/qq_emoji/277/png/277.png';
    const canvas = document.createElement('canvas');
    canvas.width = 640;
    canvas.height = 1800;
    const context = canvas.getContext('2d');
    if (!context) return '';
    context.fillStyle = '#fffaf4';
    context.fillRect(0, 0, 640, 1800);
    context.fillStyle = '#272b35';
    context.font = 'bold 36px sans-serif';
    context.fillText('长图预览', 44, 70);
    context.font = '24px sans-serif';
    context.fillStyle = '#657080';
    context.fillText('合成测试图片 · 缩放后可拖动查看', 44, 112);
    for (let index = 0; index < 14; index++) {
        const top = 160 + index * 110;
        context.fillStyle = index % 2 ? '#edf3f6' : '#f6ebed';
        context.fillRect(32, top, 576, 90);
        context.fillStyle = '#364152';
        context.font = '26px sans-serif';
        context.fillText(
            String(index + 1).padStart(2, '0') + '  这一行用来验证长图文字可读性',
            48,
            top + 38,
        );
        context.font = '20px sans-serif';
        context.fillText('适应窗口、原图、缩放与拖动', 98, top + 68);
    }
    context.fillStyle = '#657080';
    context.fillText('图片底部', 44, 1750);
    return canvas.toDataURL('image/png');
}
if (mediaPreview) {
    mediaPreview[mediaPreview.length - 2].message = [
        {
            type: 'markdown',
            data: {
                data: JSON.stringify({
                    content:
                        '## 讨论纪要\n\n- 群文件按后缀区分\n- 普通表情保持行内大小\n\n**下次继续确认细节。**',
                }),
            },
        },
    ];
    mediaPreview[mediaPreview.length - 3].message = [
        {
            type: 'image',
            data: {
                file: 'https://koishi.js.org/QFace/assets/qq_emoji/379/apng/379.png',
                url: 'https://koishi.js.org/QFace/assets/qq_emoji/379/apng/379.png',
                summary: '[动画表情]',
            },
        },
    ];
    mediaPreview[mediaPreview.length - 5].message = [
        { type: 'text', data: { text: '普通内联表情与明确标记的超级表情：' } },
        { type: 'face', data: { id: '277', raw: { faceType: 1 } } },
        { type: 'face', data: { id: '379', raw: { faceType: 3, faceText: '[比心]' } } },
        { type: 'text', data: { text: '[比心]' } },
    ];
    mediaPreview[mediaPreview.length - 6].message = [
        { type: 'image', data: { file: 'preview-stored-image', file_id: 'preview-stored-image' } },
    ];
    mediaPreview[mediaPreview.length - 7].message = [
        { type: 'face', data: { id: '498' } },
        { type: 'text', data: { text: '[中!]' } },
    ];
    mediaPreview[mediaPreview.length - 8].message = [
        { type: 'face', data: { id: '494' } },
        { type: 'text', data: { text: '[举杯邀月]' } },
    ];
    mediaPreview[mediaPreview.length - 9].message = [
        { type: 'face', data: { id: '495' } },
        { type: 'text', data: { text: '[兔来]' } },
    ];
    mediaPreview[mediaPreview.length - 1].message = [
        { type: 'text', data: { text: '媒体预览：语音失败与重试' } },
        { type: 'record', data: { file: 'preview-error' } },
    ];
    mediaPreview[mediaPreview.length - 4].message = [
        {
            type: 'file',
            data: {
                file: 'NapCat 部署说明.pdf',
                file_id: 'mock-file-1',
                file_size: '2480312',
                busid: 102,
            },
        },
    ];
}
export const chatMock = {
    targets: () => onebotDebugMock.targets(),
    async call(
        request: DebugCallRequest,
        onProgress?: (progress: DebugStreamProgress) => void,
    ): Promise<DebugCallResponse> {
        const params = request.params as Record<string, unknown>;
        let data: unknown = null;
        const messageId = ++seq;
        if (request.action === 'upload_group_file' || request.action === 'upload_private_file') {
            const name = String(params.name || '文件');
            const size = 3_200_000;
            try {
                await chatGroupFilesMock.transfer(
                    request.request_id,
                    name,
                    size,
                    'uploading',
                    onProgress,
                );
            } catch {
                return {
                    request_id: request.request_id,
                    result: { kind: 'err', error: { kind: 'cancelled' } },
                } satisfies DebugCallResponse;
            }
            if (params.group_id)
                chatGroupFilesMock.addUpload(
                    String(params.group_id),
                    String(params.folder_id ?? '/'),
                    name,
                    size,
                );
            data = { file_id: `mock-upload-${messageId}` };
        } else if (request.action === 'get_group_member_info')
            data = {
                group_id: params.group_id,
                user_id: params.user_id,
                role:
                    String(params.user_id) === '10021'
                        ? 'member'
                        : String(params.user_id) === '10022'
                          ? 'admin'
                          : 'owner',
                nickname:
                    friends.find((friend) => String(friend.user_id) === String(params.user_id))
                        ?.nickname ?? '预览账号',
                shut_up_timestamp:
                    mutedMembers.get(`${request.bot_id}:${params.group_id}:${params.user_id}`) ?? 0,
            };
        else if (request.action === 'get_group_list') data = groups;
        else if (request.action === 'get_friend_list') data = friends;
        else if (request.action === 'get_friends_with_category')
            data = [
                {
                    categoryId: 1,
                    categoryName: '开发伙伴',
                    categoryMbCount: 1,
                    buddyList: [friends[0]],
                },
                {
                    categoryId: 2,
                    categoryName: '生活朋友',
                    categoryMbCount: 1,
                    buddyList: [friends[1]],
                },
            ];
        else if (request.action === 'get_group_member_list')
            data = friends
                .filter(
                    (friend) =>
                        !removedMembers
                            .get(`${request.bot_id}:${params.group_id}`)
                            ?.has(String(friend.user_id)),
                )
                .map((friend) => ({
                    ...friend,
                    group_id: params.group_id,
                    card: friend.nickname,
                    role: friend.user_id === 10022 ? 'admin' : 'member',
                    sex: friend.user_id === 10022 ? 'female' : 'male',
                    age: friend.user_id === 10022 ? 25 : 24,
                    level: '12',
                    join_time: baseTime - 86400 * 200,
                    last_sent_time: baseTime,
                    shut_up_timestamp:
                        mutedMembers.get(
                            `${request.bot_id}:${params.group_id}:${friend.user_id}`,
                        ) ?? 0,
                }));
        else if (request.action === 'set_group_ban') {
            mutedMembers.set(
                `${request.bot_id}:${params.group_id}:${params.user_id}`,
                Number(params.duration)
                    ? Math.floor(Date.now() / 1000) + Number(params.duration)
                    : 0,
            );
            data = null;
        } else if (request.action === 'set_group_kick') {
            const key = `${request.bot_id}:${params.group_id}`;
            const removed = removedMembers.get(key) ?? new Set<string>();
            removed.add(String(params.user_id));
            removedMembers.set(key, removed);
            data = null;
        } else if (request.action === 'get_group_info')
            data = {
                ...(groups.find((group) => String(group.group_id) === String(params.group_id)) ?? {
                    group_id: params.group_id,
                    group_name: '预览群',
                    member_count: 2,
                }),
                max_member_count: 500,
                group_create_time: baseTime - 86400 * 900,
                group_remark: '一起讨论与记录',
                group_level: 3,
            };
        else if (request.action === 'get_stranger_info')
            data = {
                ...(friends.find((friend) => String(friend.user_id) === String(params.user_id)) ?? {
                    user_id: params.user_id,
                    nickname: '预览好友',
                }),
                sex: 'female',
                age: 25,
                qqLevel: 36,
                long_nick: '认真生活，也认真记录。',
                country: '中国',
                province: '浙江',
                city: '杭州',
            };
        else if (request.action === 'fetch_custom_face')
            data = favorites(request.bot_id).map((face) => face.url);
        else if (request.action === 'fetch_custom_face_detail') data = favorites(request.bot_id);
        else if (request.action === 'download_file') {
            if (!/^https?:\/\//i.test(String(params.url))) return failed(request, '图片资源无效');
            const path = `C:\\preview-cache\\face-${++seq}.png`;
            downloadedFaces.set(`${request.bot_id}:${path}`, String(params.url));
            data = { file: path };
        } else if (request.action === 'add_custom_face') {
            const file = String(params.file ?? '');
            const url =
                downloadedFaces.get(`${request.bot_id}:${file}`) ??
                (/^(https?:\/\/|data:image\/)/i.test(file) ? file : '');
            if (!url) return failed(request, '找不到要收藏的图片');
            const faces = favorites(request.bot_id);
            if (!faces.some((face) => face.url === url)) faces.push({ url, desc: '新收藏' });
            data = null;
        } else if (request.action === 'fetch_sys_faces')
            data = {
                packs: [
                    {
                        pack_name: 'QQ 表情',
                        emojis: QQ_FACE_FALLBACK.map((face) => ({
                            q_sid: face.id,
                            q_des: face.name,
                            is_super: false,
                        })),
                    },
                ],
            };
        else if (request.action === 'delete_msg') {
            const entry = [...histories.entries()].find(([, rows]) =>
                rows.some((row) => String(row.message_id) === String(params.message_id)),
            );
            const owner = messageOwners.get(String(params.message_id));
            if (!entry || (owner && owner !== request.bot_id))
                return failed(request, '原消息不存在或不属于当前账号');
            if (entry) {
                const [session, rows] = entry;
                const row = rows.find(
                    (item) => String(item.message_id) === String(params.message_id),
                )!;
                const [type, peer] = session.split(':');
                for (const sub of subscriptions.values())
                    if (sub.bot === request.bot_id)
                        sub.send({
                            v: 1,
                            bot_id: request.bot_id,
                            events: [
                                {
                                    seq: ++seq,
                                    at_ms: Date.now(),
                                    body: {
                                        kind: 'ob11',
                                        payload: {
                                            post_type: 'notice',
                                            notice_type:
                                                type === 'group' ? 'group_recall' : 'friend_recall',
                                            ...(type === 'group'
                                                ? { group_id: peer }
                                                : { user_id: peer }),
                                            message_id: row.message_id,
                                        },
                                    },
                                },
                            ],
                        });
            }
        } else if (request.action === 'get_forward_msg') {
            const resource = String(params.id ?? params.message_id);
            const saved = forwardedMessages.get(`${request.bot_id}:${resource}`);
            if (resource.startsWith('preview-sent-') && !saved)
                return failed(request, '当前账号没有这条转发记录');
            data = {
                messages:
                    saved ??
                    (String(params.id ?? params.message_id) === 'preview-nested'
                        ? [
                              {
                                  sender: { user_id: 10022, nickname: '阿澄' },
                                  time: baseTime,
                                  message: [
                                      {
                                          type: 'text',
                                          data: { text: '这是一条嵌套转发中的消息。' },
                                      },
                                      { type: 'face', data: { id: '14' } },
                                  ],
                              },
                          ]
                        : [
                              {
                                  sender: { user_id: 10021, nickname: '小林' },
                                  time: baseTime - 120,
                                  message: [
                                      {
                                          type: 'text',
                                          data: { text: '这几条消息放在一起，回看更方便。' },
                                      },
                                  ],
                              },
                              {
                                  sender: { user_id: 10022, nickname: '阿澄' },
                                  time: baseTime - 60,
                                  message: [{ type: 'forward', data: { id: 'preview-nested' } }],
                              },
                          ]),
            };
        } else if (request.action === 'fetch_ptt_text')
            data = { text: '这是一段预览语音的转写内容。' };
        else if (
            request.action === 'get_image' &&
            (params.file ?? params.file_id) === 'preview-stored-image'
        )
            data = {
                url:
                    typeof window === 'undefined'
                        ? 'https://koishi.js.org/QFace/assets/qq_emoji/379/png/379.png'
                        : new URL('/qq-faces/379.png', window.location.href).href,
                file: 'C:\\NapCat\\cache\\preview-stored-image.png',
            };
        else if (request.action === 'get_image' && params.file === 'preview-long-image')
            data = { url: previewLongImage() };
        else if (request.action === 'get_file' && params.file === 'preview-video')
            data = {
                url: 'https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4',
                file_name: 'preview.mp4',
            };
        else if (request.action === 'get_record' && params.file === 'preview-tone')
            data = { base64: previewVoice(), out_format: 'wav' };
        else if (request.action === 'get_record')
            return {
                request_id: request.request_id,
                result: {
                    kind: 'ok',
                    outcome: {
                        ok: false,
                        status: 'failed',
                        retcode: 1404,
                        data: null,
                        message: '',
                        wording: '预览语音转码失败，可点击重试',
                        raw: {},
                        elapsed_ms: 1,
                        channel: { kind: 'internal' },
                        size_bytes: 0,
                        truncated: false,
                    },
                },
            };
        else if (request.action === 'get_recent_contact')
            data = [...histories.entries()].map(([key, rows]) => {
                const [type, peer] = key.split(':');
                const last = rows[rows.length - 1];
                return {
                    chatType: type === 'group' ? 2 : 1,
                    peerUin: peer,
                    peerName:
                        type === 'group'
                            ? groups.find((g) => String(g.group_id) === peer)?.group_name
                            : friends.find((f) => String(f.user_id) === peer)?.nickname,
                    msgTime: String(last.time),
                    lastestMsg: last,
                };
            });
        else if (request.action.endsWith('_msg_history')) {
            const rows =
                histories.get(
                    `${params.group_id ? 'group' : 'private'}:${params.group_id ?? params.user_id}`,
                ) ?? [];
            const cursor = params.message_seq ?? params.message_id;
            const index = cursor
                ? rows.findIndex((row) => String(row.message_id) === String(cursor))
                : rows.length;
            const end = index < 0 ? rows.length : index;
            data = { messages: rows.slice(Math.max(0, end - Number(params.count || 50)), end) };
        } else if (request.action.startsWith('send_')) {
            data = { message_id: messageId };
            const targets = await onebotDebugMock.targets();
            const self = targets.find((t) => t.bot_id === request.bot_id)?.qq_id;
            let message = params.message;
            if (request.action.endsWith('_forward_msg')) {
                const resource = `preview-sent-${messageId}`;
                const nodes = Array.isArray(params.messages) ? params.messages : [];
                if (!nodes.length || nodes.length > 20)
                    return failed(request, '请选择 1–20 条消息');
                for (const node of nodes) {
                    const value = (node as { data?: Record<string, unknown> }).data ?? {};
                    if (value.id != null) {
                        const owner = messageOwners.get(String(value.id));
                        if (
                            (owner && owner !== request.bot_id) ||
                            ![...histories.values()].some((rows) =>
                                rows.some((row) => String(row.message_id) === String(value.id)),
                            )
                        )
                            return failed(request, '原消息不存在或不属于当前账号');
                    } else if (!Array.isArray(value.content)) {
                        return failed(request, '转发节点缺少消息内容');
                    }
                }
                const rows = nodes.map((node) => {
                    const value = (node as { data?: Record<string, unknown> }).data ?? {};
                    const original =
                        value.id != null
                            ? [...histories.values()]
                                  .flat()
                                  .find((row) => String(row.message_id) === String(value.id))
                            : undefined;
                    return (
                        original || {
                            sender: {
                                user_id: value.uin || self,
                                nickname: value.name || '预览账号',
                            },
                            time: Date.now() / 1000,
                            message: value.content ?? [],
                        }
                    );
                });
                forwardedMessages.set(`${request.bot_id}:${resource}`, rows);
                message = [{ type: 'forward', data: { id: resource } }];
                data = { message_id: messageId, forward_id: resource, res_id: resource };
            }
            const payload = {
                post_type: 'message_sent',
                message_type: params.group_id ? 'group' : 'private',
                group_id: params.group_id,
                target_id: params.user_id,
                user_id: self,
                sender: { user_id: self, nickname: '预览账号' },
                message_id: messageId,
                time: Date.now() / 1000,
                message,
            };
            const session = `${params.group_id ? 'group' : 'private'}:${params.group_id ?? params.user_id}`;
            const rows = histories.get(session) ?? [];
            rows.push(payload);
            histories.set(session, rows);
            messageOwners.set(String(messageId), request.bot_id);
            for (const sub of subscriptions.values())
                if (sub.bot === request.bot_id)
                    sub.send({
                        v: 1,
                        bot_id: request.bot_id,
                        events: [
                            {
                                seq: ++seq,
                                at_ms: Date.now(),
                                body: {
                                    kind: 'ob11',
                                    payload,
                                },
                            },
                        ],
                    });
        }
        return {
            request_id: request.request_id,
            result: {
                kind: 'ok',
                outcome: {
                    ok: true,
                    status: 'ok',
                    retcode: 0,
                    data,
                    message: '',
                    wording: '',
                    raw: {},
                    elapsed_ms: 15,
                    channel: { kind: 'internal' },
                    size_bytes: 0,
                    truncated: false,
                },
            },
        };
    },
    async subscribe(
        bot: string,
        send: (batch: DebugEventBatch) => void,
    ): Promise<DebugSubscribeResponse> {
        const subscription_id = crypto.randomUUID();
        subscriptions.set(subscription_id, { bot, send });
        const rows = [...histories.values()].map((rows) => rows[rows.length - 1]);
        send({
            v: 1,
            bot_id: bot,
            events: rows.map((payload) => ({
                seq: ++seq,
                at_ms: Date.now(),
                body: { kind: 'ob11' as const, payload },
            })),
        });
        return {
            subscription_id,
            receiver: {
                bot_id: bot,
                state: { state: 'connected' },
                source: { kind: 'internal' },
                buffered: rows.length,
                dropped_total: 0,
                first_seq: seq - rows.length + 1,
                viewers: 1,
            },
        };
    },
    async unsubscribe(id: string): Promise<void> {
        subscriptions.delete(id);
    },
    async cancel(requestId: string): Promise<void> {
        chatGroupFilesMock.cancel(requestId);
    },
};
