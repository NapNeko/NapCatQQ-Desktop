// 把聊天时间线上的条目变成界面上的字：时间文案、分隔线规则、列表行的类型 / 摘要、调用状态行、
// 头像地址、「填入请求」的参数计算。聊天视图、事件列表、详情弹层都用它，所以和 reduce 放在一起，
// 各处写出来的同一件事是同一句话。

import type { ChatCallState, ChatItem, SessionKey } from './chat';
import { messagePreview } from './segments';
import { channelShortLabel } from './channelCopy';
import { receiverStateCopy } from './receiverCopy';
import { parseParamsText, setParam } from './paramsText';
import type { FormField } from './schemaForm';

export type MessageItem = Extract<ChatItem, { kind: 'message' }>;

// ---------------------------------------------------------------------------
// 时间
// ---------------------------------------------------------------------------

const pad = (n: number, width = 2) => String(n).padStart(width, '0');

/** HH:mm:ss */
export function clockTime(ms: number): string {
    const d = new Date(ms);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** HH:mm:ss.SSS，列表模式要精确到毫秒 */
export function clockTimeMs(ms: number): string {
    return `${clockTime(ms)}.${pad(new Date(ms).getMilliseconds(), 3)}`;
}

function hourMinute(d: Date): string {
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 时间分隔线：「今天 14:02」「昨天 09:30」「9月28日 14:02」，跨年带年份 */
export function dayLabel(ms: number, now: number = Date.now()): string {
    const d = new Date(ms);
    const today = new Date(now);
    const hm = hourMinute(d);
    if (d.toDateString() === today.toDateString()) return `今天 ${hm}`;
    const yesterday = new Date(now);
    yesterday.setDate(today.getDate() - 1);
    if (d.toDateString() === yesterday.toDateString()) return `昨天 ${hm}`;
    const md = `${d.getMonth() + 1}月${d.getDate()}日`;
    return d.getFullYear() === today.getFullYear()
        ? `${md} ${hm}`
        : `${d.getFullYear()}年${md} ${hm}`;
}

/** 两条之间隔了超过 5 分钟（或跨了天）就插一条时间分隔线；第一条总是插 */
const TIME_GAP_MS = 5 * 60_000;

const DAY_MS = 86_400_000;
const HOUR_MS = 3_600_000;
// 时区偏移按小时缓存一份：相邻条目几乎总在同一小时里，一整屏只 new 一次 Date（夏令时切换的那一小时也对）。
// 只是给确定的计算记个答案，谁来问结果都一样，不算界面状态
let offsetHour = Number.NaN;
let offsetMs = 0;

/** 本地时间的「第几天」，整数比较就知道跨没跨天 */
function localDay(ms: number): number {
    const hour = Math.floor(ms / HOUR_MS);
    if (hour !== offsetHour) {
        offsetHour = hour;
        offsetMs = new Date(ms).getTimezoneOffset() * 60_000;
    }
    return Math.floor((ms - offsetMs) / DAY_MS);
}

export function needsTimeSeparator(prevAt: number | undefined, at: number): boolean {
    if (prevAt === undefined) return true;
    if (at - prevAt > TIME_GAP_MS) return true;
    return localDay(prevAt) !== localDay(at);
}

/** 断线缺口的时间段；跨天时带上日期，免得看成同一天 */
export function gapRange(fromMs: number, toMs: number): string {
    const sameDay = new Date(fromMs).toDateString() === new Date(toMs).toDateString();
    const from = clockTime(fromMs);
    const to = sameDay
        ? clockTime(toMs)
        : `${dayLabel(toMs).replace(/ \d\d:\d\d$/, '')} ${clockTime(toMs)}`;
    return `${from}–${to}`;
}

// ---------------------------------------------------------------------------
// 会话、头像、发送者
// ---------------------------------------------------------------------------

export function parseSessionKey(key: SessionKey): { type: 'group' | 'private'; id: number } | null {
    const sep = key.indexOf(':');
    if (sep < 0) return null;
    const type = key.slice(0, sep);
    const id = Number(key.slice(sep + 1));
    if (!Number.isFinite(id) || (type !== 'group' && type !== 'private')) return null;
    return { type, id };
}

/** QQ 头像；号不像 QQ 号（0、负数、太短）时不去拉，直接用首字 */
export function avatarUrl(id: number): string | null {
    if (!Number.isInteger(id) || id < 10000) return null;
    return `https://q1.qlogo.cn/g?b=qq&nk=${id}&s=100`;
}

/** 头像拉不到时显示的那个字：名字的第一个字符（按码点，emoji 不被劈开） */
export function initialOf(name: string): string {
    const first = Array.from(name.trim())[0];
    return first ? first.toUpperCase() : '?';
}

export const ROLE_LABEL: Record<string, string> = { owner: '群主', admin: '管理员' };

// ---------------------------------------------------------------------------
// 调用状态行（自己发的气泡下面、调用小标签）
// ---------------------------------------------------------------------------

const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/**
 * 气泡是哪个发送动作发的。调用状态里记着真正的动作（send_msg、转发都对得上，
 * 被 message_sent 合并过也还在）；没有调用状态时只能按会话类型推断。
 */
export function sendActionOf(item: MessageItem): string {
    const action = item.call?.action;
    if (action) return action;
    return item.session.startsWith('group:') ? 'send_group_msg' : 'send_private_msg';
}

export interface CallLine {
    ok: boolean;
    /** 成功：「✓ 128ms · 内部通道」；失败：「✗ retcode 1200 · 上游的说明」或「✗ 失败 · 没拿到回包的原因」 */
    text: string;
}

export function callLine(call: ChatCallState): CallLine {
    if (call.ok === false) {
        const head =
            call.retcode !== null && call.retcode !== undefined
                ? `✗ retcode ${call.retcode}`
                : '✗ 失败';
        const why = call.wording || call.error;
        return { ok: false, text: why ? `${head} · ${why}` : head };
    }
    const parts = [
        call.elapsedMs !== null && call.elapsedMs !== undefined ? `✓ ${call.elapsedMs}ms` : '✓',
    ];
    if (call.channel) parts.push(channelShortLabel(call.channel));
    return { ok: true, text: parts.join(' · ') };
}

const ORIGIN_LABEL: Record<string, string> = {
    editor: '',
    composer: '输入框',
    picker: '选择器',
    mcp: 'MCP',
    other: '其它客户端',
};

export function originLabel(origin: string): string {
    return ORIGIN_LABEL[origin] ?? origin;
}

// ---------------------------------------------------------------------------
// 列表模式的一行
// ---------------------------------------------------------------------------

const str = (v: unknown): string =>
    typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '';

export type RowTone =
    'message' | 'notice' | 'request' | 'call' | 'meta' | 'warn' | 'danger' | 'muted';

export interface ListRow {
    /** post_type/detail 那一列 */
    type: string;
    summary: string;
    tone: RowTone;
}

export function listRowOf(item: ChatItem): ListRow {
    switch (item.kind) {
        case 'message': {
            const raw = isRecord(item.raw) ? item.raw : {};
            const post = str(raw.post_type) || (item.direction === 'out' ? 'call' : 'message');
            const detail = item.session.startsWith('group:') ? 'group' : 'private';
            const failed = item.call?.ok === false;
            return {
                type: `${post}/${detail}`,
                summary: `${item.senderName}：${messagePreview(item.segments) || '（空消息）'}`,
                tone: failed ? 'danger' : 'message',
            };
        }
        case 'notice': {
            const raw = isRecord(item.raw) ? item.raw : {};
            const sub = str(raw.sub_type);
            return {
                type: `notice/${str(raw.notice_type) || '?'}${sub ? `.${sub}` : ''}`,
                summary: item.text,
                tone: 'notice',
            };
        }
        case 'request':
            return {
                type: `request/${item.requestType}`,
                summary:
                    item.requestType === 'group'
                        ? `${item.userId} 申请加群 ${item.groupId ?? ''}${item.comment ? `：${item.comment}` : ''}`
                        : `${item.userId} 请求加好友${item.comment ? `：${item.comment}` : ''}`,
                tone: 'request',
            };
        case 'call': {
            const line = callLine(item.call);
            return {
                type: `call/${item.action}`,
                summary: [line.text, item.summary].filter(Boolean).join('  '),
                tone: line.ok ? 'call' : 'danger',
            };
        }
        case 'meta': {
            const raw = isRecord(item.raw) ? item.raw : {};
            return {
                type: `meta_event/${str(raw.meta_event_type) || '?'}`,
                summary: item.text,
                tone: 'meta',
            };
        }
        case 'gap':
            return {
                type: 'gap',
                summary: `${gapRange(item.fromMs, item.toMs)} 断开期间可能漏了事件`,
                tone: 'warn',
            };
        case 'dropped':
            return {
                type: 'dropped',
                summary: `上游丢了 ${item.count} 条（接收太慢）`,
                tone: 'warn',
            };
        case 'receiver':
            return { type: 'receiver', summary: receiverStateCopy(item.state).text, tone: 'muted' };
    }
}

// ---------------------------------------------------------------------------
// 「填入请求」：把这条事件里的群号 / QQ 号 / 消息 id 填进当前标签的参数
// ---------------------------------------------------------------------------

export interface FillIds {
    group_id?: number;
    user_id?: number;
    message_id?: number;
}

/** 一条事件能拿出哪些 id */
export function idsOfItem(item: ChatItem): FillIds {
    const ids: FillIds = {};
    if (item.kind === 'message') {
        const s = parseSessionKey(item.session);
        if (s?.type === 'group') {
            ids.group_id = s.id;
            if (item.senderId) ids.user_id = item.senderId;
        } else if (s) {
            // 私聊里「对方」才是有用的号：别人发来的是发送者，自己发的是会话对象
            ids.user_id = item.direction === 'in' && item.senderId ? item.senderId : s.id;
        }
        if (item.messageId !== undefined) ids.message_id = item.messageId;
    } else if (item.kind === 'request') {
        if (item.groupId !== undefined) ids.group_id = item.groupId;
        if (item.userId) ids.user_id = item.userId;
    } else if (item.kind === 'notice') {
        const raw = isRecord(item.raw) ? item.raw : {};
        const group = Number(raw.group_id);
        const user = Number(raw.user_id);
        const msg = Number(raw.message_id);
        if (Number.isFinite(group) && group > 0) ids.group_id = group;
        if (Number.isFinite(user) && user > 0) ids.user_id = user;
        if (Number.isFinite(msg) && msg !== 0) ids.message_id = msg;
    }
    return ids;
}

const FIELD_KIND_TO_ID: Partial<Record<FormField['kind'], keyof FillIds>> = {
    group: 'group_id',
    friend: 'user_id',
    member: 'user_id',
    message_id: 'message_id',
};

export type FillPlan = { ok: true; text: string; filled: string[] } | { ok: false; reason: string };

/**
 * 算出填完之后的参数文本。认的参数：动作说明里 role 是群 / 好友 / 成员 / 消息 id 的字段，
 * 以及参数文本里已经有的 group_id / user_id / message_id 键。值的类型跟着字段走
 * （只收字符串的给字符串），说明里没有的跟着文本里原来的值走。
 */
export function planFill(paramsText: string, fields: readonly FormField[], ids: FillIds): FillPlan {
    const parsed = parseParamsText(paramsText);
    if (!parsed.ok)
        return { ok: false, reason: `当前请求的 JSON 第 ${parsed.line} 行有错，改好再填` };
    const current = parsed.value;
    const targets = new Map<string, { id: keyof FillIds; field?: FormField }>();
    for (const f of fields) {
        const id = FIELD_KIND_TO_ID[f.kind];
        if (id && ids[id] !== undefined) targets.set(f.name, { id, field: f });
    }
    for (const name of ['group_id', 'user_id', 'message_id'] as const) {
        if (targets.has(name) || ids[name] === undefined) continue;
        const field = fields.find((f) => f.name === name);
        if (field || Object.prototype.hasOwnProperty.call(current, name))
            targets.set(name, { id: name, field });
    }
    if (targets.size === 0) {
        return { ok: false, reason: '当前请求里没有能填的群号 / QQ 号 / 消息 id 参数' };
    }
    let text = paramsText;
    const filled: string[] = [];
    for (const [name, { id, field }] of targets) {
        const n = ids[id] as number;
        const existing = current[name];
        const asString = field
            ? field.acceptsString && (!field.acceptsNumber || typeof existing === 'string')
            : typeof existing === 'string';
        text = setParam(text, name, asString ? String(n) : n);
        filled.push(name);
    }
    return { ok: true, text, filled };
}

// ---------------------------------------------------------------------------
// 杂项
// ---------------------------------------------------------------------------

const sizeFormat = new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 1 });

/** 文件大小：上游给的可能是数字也可能是数字串 */
export function fileSizeLabel(v: unknown): string {
    const n =
        typeof v === 'number' ? v : typeof v === 'string' && /^\d+$/.test(v) ? Number(v) : NaN;
    if (!Number.isFinite(n) || n < 0) return '';
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${sizeFormat.format(n / 1024)} KB`;
    if (n < 1024 * 1024 * 1024) return `${sizeFormat.format(n / 1024 / 1024)} MB`;
    return `${sizeFormat.format(n / 1024 / 1024 / 1024)} GB`;
}

export const countFormat = new Intl.NumberFormat('zh-CN');

/** 事件详情和复制用的 JSON；循环引用之类的坏数据不让整个弹层崩掉 */
export function safeJson(value: unknown, space = 2): string {
    try {
        return JSON.stringify(value, null, space) ?? String(value);
    } catch {
        return String(value);
    }
}
