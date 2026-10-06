// 事件流 → 聊天时间线。
//
// 事件按批到达（每帧一批，来自 5000 条的环形缓冲），所以 reduceEvents 每批只拷一次数组，
// 每条事件只做 O(1) 的查表和追加；trim 也是每批一次。已看过的 seq 直接跳过，
// 断线重连后重放的积压不会产生重复条目。
//
// 通知事件里只有 QQ 号，所以顺手从消息事件的 sender 里记下每个号的名字（群名片优先），
// 拼通知文案时用；没见过的号照旧显示号码。

import type { DebugCallOrigin } from '../../ipc/generated/debug/DebugCallOrigin';
import type { DebugCallRecord } from '../../ipc/generated/debug/DebugCallRecord';
import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugEvent } from '../../ipc/generated/debug/DebugEvent';
import type { DebugReceiverState } from '../../ipc/generated/debug/DebugReceiverState';
import { normalizeMessage, type Segment } from './segments';

export type SessionKey = string; // 'group:<id>' | 'private:<id>'

export type ChatCallState = {
    /** 真正调的动作：气泡下面写它，不按会话类型去猜（send_msg、转发都对得上） */
    action: string;
    /** 旁路看到的别人发起的调用没有 */
    requestId: string | null;
    ok?: boolean;
    retcode?: number | null;
    elapsedMs?: number | null;
    /** 没拿到回包的原因（超时、通道不通……）；拿到了回包、只是 retcode 非 0 时为空 */
    error?: string | null;
    /**
     * 拿到了回包但 OB11 说失败时上游给的说明（wording，没有就 message）。事件里的调用记录不带它，
     * 只有本应用自己发起、手里有完整回包的调用才补得上
     */
    wording?: string;
    channel?: DebugChannelId | null;
};

export type ChatItem =
    | {
          kind: 'message';
          key: string;
          seq: number;
          at: number;
          session: SessionKey;
          direction: 'in' | 'out';
          senderId: number;
          senderName: string;
          senderRole?: string;
          messageId?: number;
          segments: Segment[];
          call?: ChatCallState;
          raw: unknown;
      }
    | {
          kind: 'notice';
          key: string;
          seq: number;
          at: number;
          session?: SessionKey;
          text: string;
          raw: unknown;
      }
    | {
          kind: 'request';
          key: string;
          seq: number;
          at: number;
          requestType: 'friend' | 'group';
          userId: number;
          groupId?: number;
          comment: string;
          flag: string;
          raw: unknown;
      }
    | {
          kind: 'call';
          key: string;
          seq: number;
          at: number;
          action: string;
          origin: DebugCallOrigin;
          call: ChatCallState;
          summary: string;
          raw: unknown;
      }
    | {
          kind: 'meta';
          key: string;
          seq: number;
          at: number;
          text: string;
          heartbeat: boolean;
          raw: unknown;
      }
    | { kind: 'gap'; key: string; seq: number; at: number; fromMs: number; toMs: number }
    | { kind: 'dropped'; key: string; seq: number; at: number; count: number }
    | { kind: 'receiver'; key: string; seq: number; at: number; state: DebugReceiverState };

export interface ChatSession {
    key: SessionKey;
    type: 'group' | 'private';
    id: number;
    name: string;
    lastAt: number;
    unread: number;
}

export interface ChatState {
    items: ChatItem[];
    sessions: Record<SessionKey, ChatSession>;
    /** message_id → 该自发气泡在 items 里的下标；调用回包和 message_sent 事件按它合并成一个气泡 */
    outByMessageId: Record<number, number>;
    /**
     * QQ 号 → 最近一次在消息里见到的名字（群名片 > 昵称）。通知文案和「请求」卡片靠它把号换成人名。
     * 按号记、不分群：同一个人在别的群的名片也会被借来用，比只给号好认。有上限，满了丢最早记下的一半
     */
    names: ReadonlyMap<number, string>;
    lastSeq: number;
    selfId?: number;
}

type MessageItem = Extract<ChatItem, { kind: 'message' }>;

export const MAX_CHAT_ITEMS = 5000;

/** 名字表的上限：几个大群一起刷屏也就几千个号，再多就是很久以前的人了 */
const MAX_NAMES = 10_000;

export function emptyChat(selfId?: number): ChatState {
    return { items: [], sessions: {}, outByMessageId: {}, names: new Map(), lastSeq: 0, selfId };
}

/** 这些动作成功后会在会话里多出一条自己发的消息 */
const SEND_ACTIONS: ReadonlySet<string> = new Set([
    'send_group_msg',
    'send_private_msg',
    'send_msg',
    'send_group_forward_msg',
    'send_private_forward_msg',
    'send_forward_msg',
]);

/** 转发类动作的参数里是 messages（节点数组）而不是 message */
const FORWARD_ACTIONS: ReadonlySet<string> = new Set([
    'send_group_forward_msg',
    'send_private_forward_msg',
    'send_forward_msg',
]);

const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/** QQ 号 / 群号 / 消息 id：数字或纯数字字符串都认（NapCat 的参数是字符串） */
function num(v: unknown): number | undefined {
    if (typeof v === 'number' && Number.isFinite(v)) return v;
    if (typeof v === 'string' && /^-?\d+$/.test(v.trim())) {
        const n = Number(v);
        return Number.isFinite(n) ? n : undefined;
    }
    return undefined;
}

const text = (v: unknown): string =>
    typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '';

/** 通知里的人：认得名字给名字，否则给号；连号都没有是「某人」 */
function who(v: unknown, names: ReadonlyMap<number, string>): string {
    const n = num(v);
    if (n === undefined) return '某人';
    return names.get(n) ?? String(n);
}

function groupKey(id: number): SessionKey {
    return `group:${id}`;
}

function privateKey(id: number): SessionKey {
    return `private:${id}`;
}

// ---------------------------------------------------------------------------
// 单条事件 → 条目
// ---------------------------------------------------------------------------

function noticeText(p: Record<string, unknown>, names: ReadonlyMap<number, string>): string {
    const type = text(p.notice_type);
    const sub = text(p.sub_type);
    const user = who(p.user_id, names);
    const op = who(p.operator_id, names);
    const userId = num(p.user_id);
    const opId = num(p.operator_id);
    switch (type) {
        case 'group_increase':
            return sub === 'invite' && opId !== undefined && opId !== userId
                ? `${op} 邀请 ${user} 加入了群`
                : `${user} 加入了群`;
        case 'group_decrease':
            // kick_me 是 Bot 自己被踢，user_id 就是 Bot
            return sub === 'kick' || sub === 'kick_me'
                ? `${op} 把 ${user} 移出了群`
                : `${user} 离开了群`;
        case 'group_ban': {
            // user_id 为 0 是全员禁言
            const whole = num(p.user_id) === 0;
            if (sub === 'lift_ban')
                return whole ? `${op} 关闭了全员禁言` : `${op} 解除了 ${user} 的禁言`;
            return whole ? `${op} 开启了全员禁言` : `${op} 禁言了 ${user} ${num(p.duration) ?? 0}s`;
        }
        case 'group_recall':
            // 自己撤回自己的只写一个人；管理员撤回别人的，写清楚撤的是谁的
            if (opId === undefined || userId === undefined || opId === userId) {
                return `${who(opId ?? userId, names)} 撤回了一条消息`;
            }
            return `${op} 撤回了 ${user} 的一条消息`;
        case 'friend_recall':
            return `${user} 撤回了一条消息`;
        case 'notify':
            if (sub === 'poke')
                return `${who(p.sender_id ?? p.user_id, names)} 戳了戳 ${who(p.target_id, names)}`;
            return sub ? `notify/${sub}` : 'notify';
        case 'group_admin':
            return sub === 'unset' ? `${user} 被取消了管理员` : `${user} 成为了管理员`;
        case 'group_upload': {
            const file = isRecord(p.file) ? text(p.file.name) : '';
            return `${user} 上传了文件 ${file}`.trimEnd();
        }
        case 'friend_add':
            return `${user} 成为了你的好友`;
        case 'essence': {
            const author = num(p.sender_id) !== undefined ? who(p.sender_id, names) : null;
            if (sub === 'delete')
                return author
                    ? `${op} 取消了 ${author} 的一条精华消息`
                    : `${op} 取消了一条精华消息`;
            return author ? `${op} 把 ${author} 的一条消息设为精华` : `${op} 把一条消息设为精华`;
        }
        case 'group_card': {
            const card = text(p.card_new);
            return card ? `${user} 的群名片改为「${card}」` : `${user} 清除了群名片`;
        }
        default:
            return sub ? `${type}/${sub}` : type;
    }
}

/** 通知归到哪个会话：有群号归群，好友类通知归私聊，其余（如自身状态变化）不归属 */
function noticeSession(p: Record<string, unknown>): SessionKey | undefined {
    const groupId = num(p.group_id);
    if (groupId !== undefined) return groupKey(groupId);
    const type = text(p.notice_type);
    if (
        type === 'friend_recall' ||
        type === 'friend_add' ||
        (type === 'notify' && text(p.sub_type) === 'poke')
    ) {
        const userId = num(p.user_id);
        if (userId !== undefined) return privateKey(userId);
    }
    return undefined;
}

function metaText(p: Record<string, unknown>): { text: string; heartbeat: boolean } {
    const type = text(p.meta_event_type);
    if (type === 'heartbeat') return { text: '心跳', heartbeat: true };
    if (type === 'lifecycle') {
        const sub = text(p.sub_type);
        const label =
            sub === 'connect'
                ? '连接建立'
                : sub === 'enable'
                  ? 'Bot 已启用'
                  : sub === 'disable'
                    ? 'Bot 已停用'
                    : sub;
        return { text: label ? `生命周期：${label}` : '生命周期', heartbeat: false };
    }
    return { text: type || '元事件', heartbeat: false };
}

function callStateOf(r: DebugCallRecord, wording: string | undefined): ChatCallState {
    const state: ChatCallState = { action: r.action, requestId: r.request_id };
    if (r.ok !== null) state.ok = r.ok;
    state.retcode = r.retcode;
    state.elapsedMs = r.elapsed_ms;
    state.error = r.error;
    if (wording) state.wording = wording;
    state.channel = r.channel;
    return state;
}

/** 参数摘要：只取顶层键，长字符串截断，避免为一个上传文件的 base64 去序列化几兆内容 */
function summarizeParams(params: unknown): string {
    if (!isRecord(params)) return '';
    const parts: string[] = [];
    for (const [k, v] of Object.entries(params)) {
        if (parts.length >= 6) {
            parts.push('…');
            break;
        }
        let shown: string;
        if (typeof v === 'string') shown = v.length > 32 ? `${v.slice(0, 32)}…` : v;
        else if (Array.isArray(v)) shown = `[${v.length} 项]`;
        else if (isRecord(v)) shown = '{…}';
        else shown = String(v);
        parts.push(`${k}=${shown}`);
    }
    const out = parts.join(' ');
    return out.length > 120 ? `${out.slice(0, 120)}…` : out;
}

/** 从 send_* 调用的参数里认出发往哪个会话；认不出返回 undefined（此时退化成普通调用条目） */
function sessionOfSend(action: string, params: unknown): SessionKey | undefined {
    if (!isRecord(params)) return undefined;
    const groupId = num(params.group_id);
    const userId = num(params.user_id);
    if (action === 'send_group_msg' || action === 'send_group_forward_msg') {
        return groupId !== undefined ? groupKey(groupId) : undefined;
    }
    if (action === 'send_private_msg' || action === 'send_private_forward_msg') {
        return userId !== undefined ? privateKey(userId) : undefined;
    }
    // send_msg / send_forward_msg：先看 message_type，缺了就看哪个 id 在
    const type = text(params.message_type);
    if (type === 'group' && groupId !== undefined) return groupKey(groupId);
    if (type === 'private' && userId !== undefined) return privateKey(userId);
    if (groupId !== undefined) return groupKey(groupId);
    if (userId !== undefined) return privateKey(userId);
    return undefined;
}

function segmentsOfSend(action: string, params: Record<string, unknown>): Segment[] {
    if (
        FORWARD_ACTIONS.has(action) &&
        params.message === undefined &&
        params.messages !== undefined
    ) {
        return [{ type: 'forward', data: { messages: params.messages } }];
    }
    return normalizeMessage(params.message);
}

// ---------------------------------------------------------------------------
// reducer
// ---------------------------------------------------------------------------

/** 一批事件里的可变工作区：第一次改动时才拷贝，整批没有变化就原样返回旧 state */
class Draft {
    items: ChatItem[];
    sessions: Record<SessionKey, ChatSession>;
    outByMessageId: Record<number, number>;
    names: ReadonlyMap<number, string>;
    lastSeq: number;
    selfId: number | undefined;
    private itemsCopied = false;
    private sessionsCopied = false;
    private outCopied = false;
    private ownNames: Map<number, string> | null = null;

    constructor(state: ChatState) {
        this.items = state.items;
        this.sessions = state.sessions;
        this.outByMessageId = state.outByMessageId;
        this.names = state.names;
        this.lastSeq = state.lastSeq;
        this.selfId = state.selfId;
    }

    /**
     * 记下一个号的名字。绝大多数消息来自认识的人、名字也没变，这时只是一次查表；
     * 真有新名字才在这一批里拷一次表（满了顺手丢掉最早记下的一半）
     */
    learnName(id: number, name: string): void {
        if (!name || this.names.get(id) === name) return;
        let own = this.ownNames;
        if (!own) {
            own = copyNames(this.names);
            this.ownNames = own;
            this.names = own;
        }
        // 先删再放：改过名的人挪到末尾，满了清理时不会先被丢掉
        own.delete(id);
        own.set(id, name);
    }

    ownItems(): ChatItem[] {
        if (!this.itemsCopied) {
            this.items = this.items.slice();
            this.itemsCopied = true;
        }
        return this.items;
    }

    ownSessions(): Record<SessionKey, ChatSession> {
        if (!this.sessionsCopied) {
            this.sessions = { ...this.sessions };
            this.sessionsCopied = true;
        }
        return this.sessions;
    }

    ownOut(): Record<number, number> {
        if (!this.outCopied) {
            this.outByMessageId = { ...this.outByMessageId };
            this.outCopied = true;
        }
        return this.outByMessageId;
    }

    /** 已有的自发气泡：下标 + 校验，trim 或别的路径出了偏差也不会串到别的条目上 */
    findOut(messageId: number): { index: number; item: MessageItem } | undefined {
        const index = this.outByMessageId[messageId];
        if (index === undefined) return undefined;
        const item = this.items[index];
        if (item && item.kind === 'message' && item.messageId === messageId) return { index, item };
        return undefined;
    }

    push(item: ChatItem): number {
        const items = this.ownItems();
        items.push(item);
        return items.length - 1;
    }

    touchSession(
        key: SessionKey,
        patch: { name?: string; at: number; unreadDelta?: number },
    ): void {
        const sessions = this.ownSessions();
        const prev = sessions[key];
        const sep = key.indexOf(':');
        const type = key.slice(0, sep) === 'group' ? 'group' : 'private';
        const id = Number(key.slice(sep + 1));
        const name = patch.name || prev?.name || (type === 'group' ? `群 ${id}` : String(id));
        sessions[key] = {
            key,
            type,
            id,
            name,
            lastAt: Math.max(prev?.lastAt ?? 0, patch.at),
            unread: (prev?.unread ?? 0) + (patch.unreadDelta ?? 0),
        };
    }

    /** 超出上限就丢最早的；outByMessageId 的下标整体前移，指向被丢条目的删掉 */
    trim(): void {
        const overflow = this.items.length - MAX_CHAT_ITEMS;
        if (overflow <= 0) return;
        if (this.itemsCopied) this.items.splice(0, overflow);
        else {
            this.items = this.items.slice(overflow);
            this.itemsCopied = true;
        }
        const out = this.ownOut();
        for (const [id, index] of Object.entries(out)) {
            if (index < overflow) delete out[Number(id)];
            else out[Number(id)] = index - overflow;
        }
    }
}

function copyNames(src: ReadonlyMap<number, string>): Map<number, string> {
    if (src.size < MAX_NAMES) return new Map(src);
    const out = new Map<number, string>();
    let skip = src.size - MAX_NAMES / 2;
    for (const [id, name] of src) {
        if (skip > 0) skip -= 1;
        else out.set(id, name);
    }
    return out;
}

function reduceMessage(
    d: Draft,
    e: DebugEvent,
    p: Record<string, unknown>,
    active: SessionKey | 'all',
): void {
    const sent = p.post_type === 'message_sent';
    const senderObj = isRecord(p.sender) ? p.sender : {};
    const userId = num(p.user_id) ?? num(senderObj.user_id) ?? 0;
    if (d.selfId === undefined) {
        const self = num(p.self_id);
        if (self !== undefined) d.selfId = self;
    }
    const direction: 'in' | 'out' =
        sent || (d.selfId !== undefined && userId === d.selfId) ? 'out' : 'in';

    const isGroup =
        p.message_type === 'group' ||
        (p.message_type === undefined && num(p.group_id) !== undefined);
    // 自己发的私聊，对话对象是 target_id；别人发来的，对话对象是发送者
    const session: SessionKey = isGroup
        ? groupKey(num(p.group_id) ?? 0)
        : privateKey(direction === 'out' ? (num(p.target_id) ?? userId) : userId);

    const nickname = text(senderObj.card) || text(senderObj.nickname);
    const senderName = nickname || String(userId);
    if (nickname && userId) d.learnName(userId, nickname);
    const messageId = num(p.message_id);
    const segments = normalizeMessage(p.message !== undefined ? p.message : p.raw_message);
    const role = text(senderObj.role);

    const built: MessageItem = {
        kind: 'message',
        key: `e${e.seq}`,
        seq: e.seq,
        at: e.at_ms,
        session,
        direction,
        senderId: userId,
        senderName,
        ...(role ? { senderRole: role } : {}),
        ...(messageId !== undefined ? { messageId } : {}),
        segments,
        raw: p,
    };

    // 自发消息可能已经由调用回包先建了气泡：把事件里的内容并进去，保留原来的位置和调用状态
    const existing =
        direction === 'out' && messageId !== undefined ? d.findOut(messageId) : undefined;
    if (existing) {
        const { item: prev, index } = existing;
        d.ownItems()[index] = {
            ...built,
            key: prev.key,
            seq: prev.seq,
            at: prev.at,
            ...(prev.call ? { call: prev.call } : {}),
        };
    } else {
        const index = d.push(built);
        if (direction === 'out' && messageId !== undefined) d.ownOut()[messageId] = index;
    }

    // 会话名：群取上游给的群名，私聊取对方昵称；拿不到就沿用已有的名字
    const name = isGroup ? text(p.group_name) : direction === 'in' ? nickname : '';
    // 只有别人发来、且不在当前会话的消息才算未读；正在看「全部」时所有消息都在眼前
    const unread = direction === 'in' && active !== 'all' && session !== active ? 1 : 0;
    d.touchSession(session, { name, at: e.at_ms, unreadDelta: unread });
}

function reduceCall(
    d: Draft,
    e: DebugEvent,
    r: DebugCallRecord,
    callWording: CallWording | undefined,
): void {
    const wording =
        r.ok === false && r.request_id !== null && callWording
            ? callWording(r.request_id)
            : undefined;
    const call = callStateOf(r, wording);
    if (SEND_ACTIONS.has(r.action) && r.ok !== null) {
        const session = sessionOfSend(r.action, r.params);
        const params = isRecord(r.params) ? r.params : {};
        const messageId = r.message_id ?? undefined;
        if (session && r.ok && messageId !== undefined) {
            const existing = d.findOut(messageId);
            if (existing) {
                // message_sent 先到：只补上调用状态
                d.ownItems()[existing.index] = { ...existing.item, call };
            } else {
                const index = d.push({
                    kind: 'message',
                    key: `e${e.seq}`,
                    seq: e.seq,
                    at: e.at_ms,
                    session,
                    direction: 'out',
                    senderId: d.selfId ?? 0,
                    senderName: '我',
                    messageId,
                    segments: segmentsOfSend(r.action, params),
                    call,
                    raw: r,
                });
                d.ownOut()[messageId] = index;
            }
            d.touchSession(session, { at: e.at_ms });
            return;
        }
        if (session && !r.ok) {
            // 发送失败没有 message_id，永远等不到对应的 message_sent，单独成一个标红的气泡
            d.push({
                kind: 'message',
                key: `e${e.seq}`,
                seq: e.seq,
                at: e.at_ms,
                session,
                direction: 'out',
                senderId: d.selfId ?? 0,
                senderName: '我',
                segments: segmentsOfSend(r.action, params),
                call,
                raw: r,
            });
            d.touchSession(session, { at: e.at_ms });
            return;
        }
    }
    d.push({
        kind: 'call',
        key: `e${e.seq}`,
        seq: e.seq,
        at: e.at_ms,
        action: r.action,
        origin: r.origin,
        call,
        summary: summarizeParams(r.params),
        raw: r,
    });
}

function reduceOb11(
    d: Draft,
    e: DebugEvent,
    p: Record<string, unknown>,
    active: SessionKey | 'all',
): void {
    switch (p.post_type) {
        case 'message':
        case 'message_sent':
            reduceMessage(d, e, p, active);
            return;
        case 'notice': {
            const session = noticeSession(p);
            d.push({
                kind: 'notice',
                key: `e${e.seq}`,
                seq: e.seq,
                at: e.at_ms,
                ...(session ? { session } : {}),
                text: noticeText(p, d.names),
                raw: p,
            });
            // 改了群名片：这一条的文案用的还是旧名字，之后的通知换成新的
            if (p.notice_type === 'group_card') {
                const userId = num(p.user_id);
                const card = text(p.card_new);
                if (userId && card) d.learnName(userId, card);
            }
            break;
        }
        case 'request': {
            const requestType = p.request_type === 'group' ? 'group' : 'friend';
            const groupId = num(p.group_id);
            d.push({
                kind: 'request',
                key: `e${e.seq}`,
                seq: e.seq,
                at: e.at_ms,
                requestType,
                userId: num(p.user_id) ?? 0,
                ...(groupId !== undefined ? { groupId } : {}),
                comment: text(p.comment),
                flag: text(p.flag),
                raw: p,
            });
            break;
        }
        case 'meta_event': {
            const meta = metaText(p);
            d.push({ kind: 'meta', key: `e${e.seq}`, seq: e.seq, at: e.at_ms, ...meta, raw: p });
            break;
        }
        default:
            // 不认识的 post_type 也留一条，调试台不能悄悄吞事件
            d.push({
                kind: 'meta',
                key: `e${e.seq}`,
                seq: e.seq,
                at: e.at_ms,
                text: `未知事件 ${text(p.post_type) || '(无 post_type)'}`,
                heartbeat: false,
                raw: p,
            });
    }
}

/** 按 request_id 问一句：这次失败的调用有没有上游给的说明 */
export type CallWording = (requestId: string) => string | undefined;

export interface ReduceOptions {
    activeSession: SessionKey | 'all';
    /**
     * 事件里的调用记录不带 OB11 的 wording；本应用发起的调用由发起方拿完整回包补。
     * 回包先回来时说明先记在调用方那里，reduce 到那条失败的调用时来取（每条调用只问一次）
     */
    callWording?: CallWording;
}

/**
 * 吃一批事件，给出新 state。seq 不大于 lastSeq 的直接跳过，同一批重复喂进来结果不变；
 * 整批都被跳过时原样返回传入的 state（引用不变，React 不会重渲染）。
 * 「未读」只统计别人发来、且不在当前会话里的消息；正在看「全部」时所有消息都在眼前，不计未读。
 */
export function reduceEvents(
    state: ChatState,
    events: DebugEvent[],
    opts: ReduceOptions,
): ChatState {
    const d = new Draft(state);
    for (const e of events) {
        if (e.seq <= d.lastSeq) continue;
        d.lastSeq = e.seq;
        const body = e.body;
        switch (body.kind) {
            case 'ob11':
                reduceOb11(d, e, body.payload, opts.activeSession);
                break;
            case 'call':
                reduceCall(d, e, body.record, opts.callWording);
                break;
            case 'gap':
                d.push({
                    kind: 'gap',
                    key: `e${e.seq}`,
                    seq: e.seq,
                    at: e.at_ms,
                    fromMs: body.from_ms,
                    toMs: body.to_ms,
                });
                break;
            case 'dropped':
                d.push({
                    kind: 'dropped',
                    key: `e${e.seq}`,
                    seq: e.seq,
                    at: e.at_ms,
                    count: body.count,
                });
                break;
            case 'receiver':
                d.push({
                    kind: 'receiver',
                    key: `e${e.seq}`,
                    seq: e.seq,
                    at: e.at_ms,
                    state: body.state,
                });
                break;
            default: {
                // 后端加了新的事件体而前端没跟上时，这里会编译报错
                const _exhaustive: never = body;
                void _exhaustive;
            }
        }
    }
    // 整批都是看过的 seq：原样返回，引用不变
    if (d.lastSeq === state.lastSeq) return state;
    d.trim();
    return {
        items: d.items,
        sessions: d.sessions,
        outByMessageId: d.outByMessageId,
        names: d.names,
        lastSeq: d.lastSeq,
        selfId: d.selfId,
    };
}

/**
 * 回包比事件晚到：那条失败的调用已经在时间线上了，把上游的说明补上去。
 * 从末尾往前找（刚发生的调用总在末尾附近）；刷屏时它可能已经被挤到几千条之前，
 * 所以不设更短的截断，列表本身有上限，最坏也就扫一遍 MAX_CHAT_ITEMS。
 * 找不到、不是失败的调用、或者已经是这句话时原样返回。
 */
export function withCallWording(state: ChatState, requestId: string, wording: string): ChatState {
    const items = state.items;
    for (let i = items.length - 1; i >= 0; i -= 1) {
        const item = items[i];
        if (!item || (item.kind !== 'message' && item.kind !== 'call')) continue;
        const call = item.call;
        if (!call || call.requestId !== requestId) continue;
        if (call.ok !== false || call.wording === wording) return state;
        const next = items.slice();
        next[i] = { ...item, call: { ...call, wording } };
        return { ...state, items: next };
    }
    return state;
}

/** 把某个会话的未读清零（用户点开它时） */
export function markSessionRead(state: ChatState, key: SessionKey): ChatState {
    const s = state.sessions[key];
    if (!s || s.unread === 0) return state;
    return { ...state, sessions: { ...state.sessions, [key]: { ...s, unread: 0 } } };
}
