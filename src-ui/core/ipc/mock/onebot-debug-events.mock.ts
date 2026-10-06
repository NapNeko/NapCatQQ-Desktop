// 调试台浏览器预览用的 OB11 事件生成：伪随机数、人物 / 群 / 好友名单、各类事件的载荷形状。
// 只产纯数据，不碰定时器和订阅，运行期的推送在 onebot-debug.mock.ts。
//
// NapCat 和 SnowLuma 的事件形状有几处不同（NapCat 的消息事件带很大的 `raw`、私聊带 `target_id`、
// 戳一戳带 `raw_info`），两边都生成一份，界面才能同时走查两种数据。

import type { BackendType } from '../generated/domain/BackendType';

export type Rng = () => number;

/** 种子固定的伪随机数，同一个种子永远给同一串数 */
export function mulberry32(seed: number): Rng {
    let a = seed >>> 0;
    return () => {
        a = (a + 0x6d2b79f5) >>> 0;
        let t = a;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

/** 字符串到种子，给每个 Bot 各自一串数 */
export function seedFromString(text: string): number {
    let h = 2166136261;
    for (let i = 0; i < text.length; i += 1) {
        h ^= text.charCodeAt(i);
        h = Math.imul(h, 16777619);
    }
    return h >>> 0;
}

/** 闭区间取整数 */
export function randInt(rng: Rng, min: number, max: number): number {
    return min + Math.floor(rng() * (max - min + 1));
}

export function pick<T>(rng: Rng, items: readonly T[]): T {
    return items[Math.floor(rng() * items.length)] as T;
}

// ---------------------------------------------------------------------------
// 名单
// ---------------------------------------------------------------------------

export const GROUP_COUNT = 40;
export const FRIEND_COUNT = 60;
export const BIG_GROUP_MEMBER_COUNT = 500;
/** 群号从这里往上排：测试群 n 的群号是 GROUP_ID_BASE + n */
export const GROUP_ID_BASE = 100000;
/** 好友和群成员的 QQ 号从这里往上排：第 n 个人是 USER_ID_BASE + n */
export const USER_ID_BASE = 10000;

export interface MockPerson {
    id: number;
    nickname: string;
    card: string;
    role: 'owner' | 'admin' | 'member';
}

const NAME_POOL = [
    '小明',
    'Alice',
    '阿强',
    '雪之下',
    'Bob',
    '林小满',
    '夏目',
    'Kyle',
    '苏南',
    '老王',
    'Mia',
    '陈默',
    '星野',
    '周周',
    'Leo',
    '晚风',
    '一只喵',
    'Zoe',
    '北辰',
    '阿福',
];

const CARD_POOL = ['', '', '', '管理员', '', '前端小能手', '', '后端搬砖', '', '摸鱼中'];

/** 第 n 个人（n 从 1 起）：好友列表和大群成员共用这一套 */
export function personAt(n: number): MockPerson {
    const base = NAME_POOL[(n - 1) % NAME_POOL.length] as string;
    const round = Math.floor((n - 1) / NAME_POOL.length);
    const nickname = round === 0 ? base : `${base}${round + 1}`;
    const card = CARD_POOL[n % CARD_POOL.length] as string;
    const role = n === 1 ? 'owner' : n <= 4 ? 'admin' : 'member';
    return {
        id: USER_ID_BASE + n,
        nickname,
        card: card === '管理员' ? `${nickname}·管理` : card,
        role,
    };
}

/** 群里说话的五个人，同时也是好友里的前五位 */
export const CHAT_PEOPLE: readonly MockPerson[] = [1, 2, 3, 4, 5].map(personAt);

/** 会收到事件的三个群 */
export const CHAT_GROUP_IDS: readonly number[] = [1, 2, 3].map((n) => GROUP_ID_BASE + n);

export function groupNameOf(groupId: number): string {
    return `测试群 ${groupId - GROUP_ID_BASE}`;
}

export function isKnownGroup(groupId: number): boolean {
    const n = groupId - GROUP_ID_BASE;
    return Number.isInteger(n) && n >= 1 && n <= GROUP_COUNT;
}

/** 群成员数：群 1 固定 500，其余按群号错开，看得出规模差别 */
export function memberCountOf(groupId: number): number {
    const n = groupId - GROUP_ID_BASE;
    return n === 1 ? BIG_GROUP_MEMBER_COUNT : 18 + ((n * 7) % 60);
}

// ---------------------------------------------------------------------------
// 消息段
// ---------------------------------------------------------------------------

export interface Ob11Segment {
    type: string;
    data: Record<string, unknown>;
}

const escapeCqText = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/\[/g, '&#91;').replace(/\]/g, '&#93;');
const escapeCqValue = (s: string) => escapeCqText(s).replace(/,/g, '&#44;');
const unescapeCq = (s: string) =>
    s.replace(/&#91;/g, '[').replace(/&#93;/g, ']').replace(/&#44;/g, ',').replace(/&amp;/g, '&');

/** 消息段转 CQ 码文本，对应 OB11 的 `raw_message` */
export function toRawMessage(segments: Ob11Segment[]): string {
    return segments
        .map((seg) => {
            if (seg.type === 'text') return escapeCqText(String(seg.data.text ?? ''));
            const params = Object.entries(seg.data)
                .map(([k, v]) => `${k}=${escapeCqValue(String(v))}`)
                .join(',');
            return params ? `[CQ:${seg.type},${params}]` : `[CQ:${seg.type}]`;
        })
        .join('');
}

/** CQ 码文本转消息段：NapCat 的内部通道固定按数组格式上报，字符串消息发出去后就是这个形状 */
export function cqToSegments(text: string): Ob11Segment[] {
    const out: Ob11Segment[] = [];
    const re = /\[CQ:([A-Za-z_]+)((?:,[^\]]*)?)\]/g;
    let last = 0;
    for (let m = re.exec(text); m; m = re.exec(text)) {
        if (m.index > last)
            out.push({ type: 'text', data: { text: unescapeCq(text.slice(last, m.index)) } });
        const data: Record<string, unknown> = {};
        for (const pair of (m[2] ?? '').split(',').filter(Boolean)) {
            const eq = pair.indexOf('=');
            if (eq > 0) data[pair.slice(0, eq)] = unescapeCq(pair.slice(eq + 1));
        }
        out.push({ type: m[1] as string, data });
        last = m.index + m[0].length;
    }
    if (last < text.length)
        out.push({ type: 'text', data: { text: unescapeCq(text.slice(last)) } });
    return out;
}

/** 调用参数里的 message（数组 / 单个段 / 字符串）统一成消息段数组 */
export function messageToSegments(message: unknown): Ob11Segment[] {
    if (typeof message === 'string') return cqToSegments(message);
    if (Array.isArray(message)) {
        return message
            .filter((s): s is Record<string, unknown> => typeof s === 'object' && s !== null)
            .map((s) => ({
                type: String(s.type ?? 'text'),
                data:
                    typeof s.data === 'object' && s.data !== null
                        ? (s.data as Record<string, unknown>)
                        : {},
            }));
    }
    if (typeof message === 'object' && message !== null) return messageToSegments([message]);
    return [];
}

/** 离线也能显示的占位图：按种子换底色的小 SVG，走 data URI，预览里不发任何网络请求 */
export function placeholderImage(seed: number, width = 320, height = 200): string {
    const hue = (seed * 47) % 360;
    const svg =
        `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">` +
        `<rect width="100%" height="100%" fill="hsl(${hue},55%,78%)"/>` +
        `<circle cx="${width * 0.3}" cy="${height * 0.4}" r="${height * 0.16}" fill="hsl(${(hue + 40) % 360},60%,92%)"/>` +
        `<path d="M0 ${height} L${width * 0.4} ${height * 0.55} L${width * 0.65} ${height * 0.8} L${width * 0.8} ${height * 0.62} L${width} ${height}Z" fill="hsl(${(hue + 200) % 360},40%,55%)"/>` +
        `</svg>`;
    return `data:image/svg+xml;base64,${btoa(svg)}`;
}

// ---------------------------------------------------------------------------
// 事件
// ---------------------------------------------------------------------------

export interface EventContext {
    backend: BackendType;
    selfId: number;
    selfName: string;
    rng: Rng;
    nextMessageId: () => number;
    /** 最近的群消息 id，回复和撤回要引用真实存在的消息 */
    recentGroupMessageIds: () => number[];
}

const TEXT_POOL = [
    '大家早上好',
    '有人在吗',
    '这个接口怎么调用？',
    '哈哈哈哈哈',
    '收到，我看一下',
    '今晚开黑吗',
    '刚才那个报错：retcode 1400，参数不对',
    '发个图看看',
    '@ 一下管理员，帮忙看下',
    '好的，没问题',
    '我这边复现不了',
    '有人试过新版本吗',
    '周末一起出去玩吧',
    '这个功能真好用',
    '等我一下，马上到',
    '先这样，明天再说',
];

const REQUEST_COMMENTS = [
    '我是群里的阿强，加一下',
    '看到你在测试群，想交个朋友',
    '来自开发者交流群',
    '请通过一下，谢谢',
];

const text = (t: string): Ob11Segment => ({ type: 'text', data: { text: t } });

function senderOf(person: MockPerson, groupMessage: boolean): Record<string, unknown> {
    return groupMessage
        ? { user_id: person.id, nickname: person.nickname, card: person.card, role: person.role }
        : { user_id: person.id, nickname: person.nickname, card: '' };
}

/** NapCat 的消息事件带一份很大的原始消息，前端要照样扛得住 */
function napcatRaw(
    ctx: EventContext,
    messageId: number,
    person: MockPerson,
    peerId: number,
    peerName: string,
    chatType: 1 | 2,
    atMs: number,
    segments: Ob11Segment[],
): Record<string, unknown> {
    return {
        msgId: String(messageId),
        msgRandom: String(randInt(ctx.rng, 100000000, 999999999)),
        msgSeq: String(messageId % 1000000),
        cntSeq: '0',
        chatType,
        msgType: 2,
        subMsgType: 1,
        sendType: 0,
        senderUid: `u_${person.id.toString(36)}AbCdEfGhIjKlMn`,
        peerUid: chatType === 2 ? String(peerId) : `u_${peerId.toString(36)}AbCdEfGhIjKlMn`,
        channelId: '',
        guildId: '',
        guildCode: '0',
        fromUid: '0',
        fromAppid: '0',
        msgTime: String(Math.floor(atMs / 1000)),
        msgMeta: '0x',
        sendStatus: 2,
        sendRemarkName: '',
        sendMemberName: person.card,
        sendNickName: person.nickname,
        guildName: '',
        channelName: '',
        elements: segments.map((seg, index) => ({
            elementType:
                seg.type === 'text' || seg.type === 'at' ? 1 : seg.type === 'image' ? 2 : 6,
            elementId: String(index),
            extBufForUI: '0x',
            textElement:
                seg.type === 'text' || seg.type === 'at'
                    ? {
                          content:
                              seg.type === 'text'
                                  ? String(seg.data.text)
                                  : `@${String(seg.data.qq)}`,
                          atType: seg.type === 'at' ? 2 : 0,
                          atUid: seg.type === 'at' ? String(seg.data.qq) : '0',
                          atTinyId: '0',
                          atNtUid: '',
                          subElementType: 0,
                          atChannelId: '0',
                          linkInfo: null,
                          atRoleId: '0',
                          atRoleColor: 0,
                          atRoleName: '',
                          needNotify: 0,
                      }
                    : null,
            faceElement: null,
            picElement: null,
            replyElement: null,
        })),
        records: [],
        emojiLikesList: [],
        commentCnt: '0',
        directMsgFlag: 0,
        directMsgMembers: [],
        peerName,
        freqLimitInfo: null,
        editable: false,
        avatarMeta: '',
        avatarPendant: '',
        feedId: '',
        roleId: '0',
        timeStamp: '0',
        isImportMsg: false,
        atType: 0,
        roleType: 0,
        fromChannelRoleInfo: { roleId: '0', name: '', color: 0 },
        fromGuildRoleInfo: { roleId: '0', name: '', color: 0 },
        levelRoleInfo: { roleId: '0', name: '', color: 0 },
        recallTime: '0',
        isOnlineMsg: true,
        generalFlags: '0x',
        clientSeq: '0',
        fileGroupSize: null,
        foldingInfo: null,
        multiTransInfo: null,
        senderUin: String(person.id),
        peerUin: String(peerId),
        msgAttrs: {},
        anonymousExtInfo: null,
        nameType: 0,
        avatarFlag: 0,
        extInfoForUI: null,
        personalMedal: null,
        categoryManage: 0,
        msgEventInfo: null,
        msgInfos: [],
    };
}

/** 一条别人发的群消息（`groupId` 和 `person` 不给就随机） */
export function makeGroupMessage(
    ctx: EventContext,
    atMs: number,
    opts: { groupId?: number; person?: MockPerson; segments?: Ob11Segment[] } = {},
): Record<string, unknown> {
    const { rng } = ctx;
    const groupId = opts.groupId ?? pick(rng, CHAT_GROUP_IDS);
    const person = opts.person ?? pick(rng, CHAT_PEOPLE);
    const segments = opts.segments ?? randomSegments(ctx);
    const messageId = ctx.nextMessageId();
    const time = Math.floor(atMs / 1000);
    const payload: Record<string, unknown> = {
        self_id: ctx.selfId,
        user_id: person.id,
        time,
        message_id: messageId,
        message_seq: messageId % 1000000,
        real_id: messageId % 1000000,
        message_type: 'group',
        sender: senderOf(person, true),
        raw_message: toRawMessage(segments),
        font: ctx.backend === 'napcat' ? 14 : 0,
        sub_type: 'normal',
        message: segments,
        post_type: 'message',
        group_id: groupId,
    };
    if (ctx.backend === 'napcat') {
        payload.real_seq = String(messageId % 1000000);
        payload.message_format = 'array';
        payload.group_name = groupNameOf(groupId);
        payload.raw = napcatRaw(
            ctx,
            messageId,
            person,
            groupId,
            groupNameOf(groupId),
            2,
            atMs,
            segments,
        );
    }
    return payload;
}

function makePrivateMessage(ctx: EventContext, atMs: number): Record<string, unknown> {
    const { rng } = ctx;
    const person = pick(rng, CHAT_PEOPLE);
    const segments: Ob11Segment[] = [text(pick(rng, TEXT_POOL))];
    const messageId = ctx.nextMessageId();
    const payload: Record<string, unknown> = {
        self_id: ctx.selfId,
        user_id: person.id,
        time: Math.floor(atMs / 1000),
        message_id: messageId,
        message_seq: messageId % 1000000,
        real_id: messageId % 1000000,
        message_type: 'private',
        sender: senderOf(person, false),
        raw_message: toRawMessage(segments),
        font: ctx.backend === 'napcat' ? 14 : 0,
        sub_type: 'friend',
        message: segments,
        post_type: 'message',
    };
    if (ctx.backend === 'napcat') {
        payload.real_seq = '0';
        payload.message_format = 'array';
        payload.target_id = ctx.selfId;
        payload.raw = napcatRaw(
            ctx,
            messageId,
            person,
            ctx.selfId,
            ctx.selfName,
            1,
            atMs,
            segments,
        );
    }
    return payload;
}

/** 自己发出去的消息（`message_sent`）：调试台调用 send_* 成功后上游会回报这一条 */
export function makeSelfMessage(
    ctx: EventContext,
    atMs: number,
    opts: {
        messageId: number;
        messageType: 'group' | 'private';
        targetId: number;
        segments: Ob11Segment[];
    },
): Record<string, unknown> {
    const self: MockPerson = { id: ctx.selfId, nickname: ctx.selfName, card: '', role: 'member' };
    const group = opts.messageType === 'group';
    const payload: Record<string, unknown> = {
        self_id: ctx.selfId,
        user_id: ctx.selfId,
        time: Math.floor(atMs / 1000),
        message_id: opts.messageId,
        message_seq: opts.messageId % 1000000,
        real_id: opts.messageId % 1000000,
        message_type: opts.messageType,
        sender: senderOf(self, group),
        raw_message: toRawMessage(opts.segments),
        font: ctx.backend === 'napcat' ? 14 : 0,
        sub_type: group ? 'normal' : 'friend',
        message: opts.segments,
        post_type: 'message_sent',
    };
    if (group) payload.group_id = opts.targetId;
    else payload.target_id = opts.targetId;
    if (ctx.backend === 'napcat') {
        payload.real_seq = '0';
        payload.message_format = 'array';
        if (group) payload.group_name = groupNameOf(opts.targetId);
        payload.raw = napcatRaw(
            ctx,
            opts.messageId,
            self,
            opts.targetId,
            group ? groupNameOf(opts.targetId) : String(opts.targetId),
            group ? 2 : 1,
            atMs,
            opts.segments,
        );
    }
    return payload;
}

/** 群里常见的几种消息形态：纯文字、@ 人、回复、图片、表情 */
function randomSegments(ctx: EventContext): Ob11Segment[] {
    const { rng } = ctx;
    const roll = rng();
    const line = pick(rng, TEXT_POOL);
    if (roll < 0.5) return [text(line)];
    if (roll < 0.66) {
        const target = rng() < 0.5 ? String(ctx.selfId) : String(pick(rng, CHAT_PEOPLE).id);
        return [{ type: 'at', data: { qq: target } }, text(` ${line}`)];
    }
    const recent = ctx.recentGroupMessageIds();
    if (roll < 0.78 && recent.length > 0) {
        return [{ type: 'reply', data: { id: String(pick(rng, recent)) } }, text(line)];
    }
    if (roll < 0.9) {
        const seed = randInt(rng, 1, 9999);
        const image: Ob11Segment = {
            type: 'image',
            data: {
                file: `${seed.toString(16)}.jpg`,
                url: placeholderImage(seed),
                summary: '[图片]',
                sub_type: 0,
                file_size: String(randInt(rng, 20000, 400000)),
            },
        };
        return rng() < 0.4 ? [text(line), image] : [image];
    }
    return [text(line), { type: 'face', data: { id: String(randInt(rng, 0, 220)) } }];
}

function makeNotice(ctx: EventContext, atMs: number): Record<string, unknown> {
    const { rng } = ctx;
    const time = Math.floor(atMs / 1000);
    const groupId = pick(rng, CHAT_GROUP_IDS);
    const roll = rng();
    if (roll < 0.34) {
        return {
            time,
            self_id: ctx.selfId,
            post_type: 'notice',
            notice_type: 'group_increase',
            sub_type: rng() < 0.5 ? 'approve' : 'invite',
            group_id: groupId,
            operator_id: pick(rng, CHAT_PEOPLE.slice(0, 4)).id,
            user_id: USER_ID_BASE + randInt(rng, 6, 400),
        };
    }
    const recent = ctx.recentGroupMessageIds();
    if (roll < 0.67) {
        const person = pick(rng, CHAT_PEOPLE);
        return {
            time,
            self_id: ctx.selfId,
            post_type: 'notice',
            notice_type: 'group_recall',
            group_id: groupId,
            user_id: person.id,
            operator_id: person.id,
            message_id: recent.length > 0 ? pick(rng, recent) : ctx.nextMessageId(),
        };
    }
    const user = pick(rng, CHAT_PEOPLE);
    const target = rng() < 0.5 ? ctx.selfId : pick(rng, CHAT_PEOPLE).id;
    const notice: Record<string, unknown> = {
        time,
        self_id: ctx.selfId,
        post_type: 'notice',
        notice_type: 'notify',
        sub_type: 'poke',
        group_id: groupId,
        user_id: user.id,
        target_id: target,
    };
    if (ctx.backend === 'napcat') {
        notice.raw_info = [
            { col: '1', nm: '', type: 'qq', uid: String(user.id) },
            { col: '1', nm: '', type: 'nor', txt: '戳了戳' },
            { col: '1', nm: '', type: 'qq', uid: String(target) },
        ];
    }
    return notice;
}

function makeFriendRequest(ctx: EventContext, atMs: number): Record<string, unknown> {
    const { rng } = ctx;
    const userId = USER_ID_BASE + randInt(rng, 1000, 9000);
    return {
        time: Math.floor(atMs / 1000),
        self_id: ctx.selfId,
        post_type: 'request',
        request_type: 'friend',
        user_id: userId,
        comment: pick(rng, REQUEST_COMMENTS),
        flag: `${userId}_${Math.floor(atMs / 1000)}_0`,
    };
}

export function makeHeartbeat(ctx: EventContext, atMs: number): Record<string, unknown> {
    return {
        time: Math.floor(atMs / 1000),
        self_id: ctx.selfId,
        post_type: 'meta_event',
        meta_event_type: 'heartbeat',
        status: { online: true, good: true },
        interval: 30000,
    };
}

export function makeLifecycle(ctx: EventContext, atMs: number): Record<string, unknown> {
    return {
        time: Math.floor(atMs / 1000),
        self_id: ctx.selfId,
        post_type: 'meta_event',
        meta_event_type: 'lifecycle',
        sub_type: 'connect',
    };
}

/** 日常事件的一份混合：以群消息为主，夹杂私聊、通知、加好友请求 */
export function makeRandomEvent(ctx: EventContext, atMs: number): Record<string, unknown> {
    const roll = ctx.rng();
    if (roll < 0.62) return makeGroupMessage(ctx, atMs);
    if (roll < 0.74) return makePrivateMessage(ctx, atMs);
    if (roll < 0.94) return makeNotice(ctx, atMs);
    return makeFriendRequest(ctx, atMs);
}

/** 由操作触发的通知（禁言、踢人、撤回），形状照 OB11 的通知事件 */
export function makeActionNotice(
    ctx: EventContext,
    atMs: number,
    notice: Record<string, unknown>,
): Record<string, unknown> {
    return {
        time: Math.floor(atMs / 1000),
        self_id: ctx.selfId,
        post_type: 'notice',
        ...notice,
    };
}
