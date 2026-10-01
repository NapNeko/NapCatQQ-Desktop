// 中栏几块共用的小判断：参数文本算不算没动过、字段值怎么显示成文字、哪些字符串是图片链接、
// 返回结构怎么压成一棵好读的树、发送按钮为什么点不了。都是纯函数，单测在 viewHelpers.test.ts。

import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugChannels } from '../../../core/ipc/generated/debug/DebugChannels';
import { findChannel } from '../../../core/domain/debug/channelPick';
import { streamChannelBlocker } from '../../../core/domain/debug/streamActions';

// 变体名剥离和目录查询的唯一实现在 core/domain/debug/catalogView（收藏 ▶ 那边也要查分级）
export { baseActionName, lookupSummary } from '../../../core/domain/debug/catalogView';

type Json = Record<string, unknown>;

const isRecord = (v: unknown): v is Json => typeof v === 'object' && v !== null && !Array.isArray(v);

/** 空文本和 `{}` 都算「什么都没填」 */
export function isBlankParams(text: string): boolean {
    const t = text.trim();
    return t === '' || t === '{}';
}

/** 参数被用户改过：不是空的，也不是编辑器按接口说明填进去的那份 */
export function paramsDirty(text: string, initial: string): boolean {
    return !isBlankParams(text) && text !== initial;
}

/** 两个参数值是不是同一份：对象、数组按 JSON 比；undefined 只和 undefined 相等 */
export function sameJson(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (a === undefined || b === undefined) return false;
    try {
        return JSON.stringify(a) === JSON.stringify(b);
    } catch {
        return false;
    }
}

/** 字段值在输入框里的样子；没填是空串，对象、数组压成一行 JSON */
export function valueText(v: unknown): string {
    if (v === undefined || v === null) return '';
    if (typeof v === 'string') return v;
    if (typeof v === 'number' || typeof v === 'boolean') return String(v);
    try {
        return JSON.stringify(v) ?? '';
    } catch {
        return String(v);
    }
}

export function prettyJson(v: unknown): string {
    try {
        return JSON.stringify(v, null, 2) ?? String(v);
    } catch {
        return String(v);
    }
}

const IMAGE_HOST = /multimedia\.nt\.qq\.com\.cn|gchat\.qpic\.cn|qpic/i;
const IMAGE_EXT = /\.(?:png|jpe?g|gif|webp|bmp|avif)(?:$|[?#])/i;

/** QQ 的图床地址，或者以图片扩展名结尾的 http(s) 地址 */
export function isImageUrl(v: unknown): v is string {
    if (typeof v !== 'string' || v.length > 4096 || !/^https?:\/\//i.test(v)) return false;
    return IMAGE_HOST.test(v) || IMAGE_EXT.test(v);
}

export type ClickableIdKey = 'group_id' | 'user_id' | 'message_id';

/** 点了这个 id 能「用它新开」哪个查询 */
export const FOLLOW_UP_ACTION: Record<ClickableIdKey, string> = {
    group_id: 'get_group_info',
    user_id: 'get_stranger_info',
    message_id: 'get_msg',
};

/** id 值：数字，或纯数字字符串（message_id 在 SnowLuma 上可能是负数） */
export function isIdValue(v: unknown): v is number | string {
    if (typeof v === 'number') return Number.isFinite(v);
    return typeof v === 'string' && /^-?\d+$/.test(v.trim());
}

/** 找出 JSON 文本里某个顶层键所在的行（从 1 起）；找不到是 null */
export function lineOfKey(text: string, key: string): number | null {
    const needle = JSON.stringify(key);
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i += 1) {
        const at = lines[i]!.indexOf(needle);
        if (at >= 0 && /^\s*:/.test(lines[i]!.slice(at + needle.length))) return i + 1;
    }
    return null;
}

// ---------------------------------------------------------------------------
// schema 展示
// ---------------------------------------------------------------------------

export const ROLE_LABEL: Record<string, string> = {
    group_id: '群号',
    user_id: 'QQ 号',
    member_id: '群成员',
    message_id: '消息 ID',
    message: '消息',
    file: '文件',
    image: '图片',
    record: '语音',
    video: '视频',
    face_id: '表情 ID',
    timestamp: '时间戳',
};

function branches(schema: Json): Json[] {
    const list = schema.anyOf ?? schema.oneOf;
    return Array.isArray(list) ? list.filter(isRecord) : [];
}

/** 给人看的类型写法：`string`、`integer | string`、`object[]`、`enum` */
export function schemaTypeText(raw: unknown, depth = 0): string {
    if (!isRecord(raw) || depth > 4) return 'any';
    const t = raw.type;
    if (typeof t === 'string') {
        if (t === 'array' && isRecord(raw.items)) return `${schemaTypeText(raw.items, depth + 1)}[]`;
        return t;
    }
    if (Array.isArray(t)) return t.filter((x) => typeof x === 'string').join(' | ') || 'any';
    if (Array.isArray(raw.enum)) return 'enum';
    if ('const' in raw) return JSON.stringify(raw.const) ?? 'const';
    const alts = branches(raw);
    if (alts.length > 0) {
        const parts = [...new Set(alts.map((b) => schemaTypeText(b, depth + 1)))];
        // 全是常量的 anyOf 就是枚举，列出一串常量反而难读
        if (alts.every((b) => 'const' in b)) return 'enum';
        return parts.join(' | ');
    }
    if (isRecord(raw.properties)) return 'object';
    return 'any';
}

/** 一个属性的角色（x-ncd-role），anyOf 分支里写的也认 */
export function schemaRole(raw: unknown): string | null {
    if (!isRecord(raw)) return null;
    if (typeof raw['x-ncd-role'] === 'string') return raw['x-ncd-role'];
    for (const b of branches(raw)) if (typeof b['x-ncd-role'] === 'string') return b['x-ncd-role'];
    return null;
}

const MAX_SIMPLIFY_DEPTH = 6;

/**
 * 把返回值的 JSON Schema 压成一棵「键 → 类型 · 说明」的树，JSON 树直接就能看：
 * 对象变成同名键的对象，数组变成只有一个元素的数组，叶子是一行字。
 */
export function simplifySchema(raw: unknown, depth = 0): unknown {
    if (!isRecord(raw)) return 'any';
    const desc = typeof raw.description === 'string' && raw.description.trim() ? raw.description.trim() : '';
    if (depth < MAX_SIMPLIFY_DEPTH) {
        if (isRecord(raw.properties)) {
            const out: Json = {};
            for (const [k, v] of Object.entries(raw.properties)) {
                Object.defineProperty(out, k, { value: simplifySchema(v, depth + 1), enumerable: true, writable: true, configurable: true });
            }
            return out;
        }
        if (isRecord(raw.items)) return [simplifySchema(raw.items, depth + 1)];
        // anyOf 里带结构的分支（常见：anyOf[object, null]）比「object | null」一行字有用
        const structured = branches(raw).find((b) => isRecord(b.properties) || isRecord(b.items));
        if (structured) return simplifySchema({ description: desc || undefined, ...structured }, depth);
    }
    const type = schemaTypeText(raw);
    return desc ? `${type} · ${desc}` : type;
}

/** JSON 树默认全展开时大概有多少行，用来给它一个刚好的高度 */
export function countTreeRows(v: unknown, cap = 400): number {
    let n = 0;
    const walk = (x: unknown) => {
        if (n >= cap) return;
        n += 1;
        if (Array.isArray(x)) x.forEach(walk);
        else if (isRecord(x)) Object.values(x).forEach(walk);
    };
    walk(v);
    return Math.min(n, cap);
}

// ---------------------------------------------------------------------------
// 消息
// ---------------------------------------------------------------------------

export function textToSegments(text: string): unknown[] {
    return text === '' ? [] : [{ type: 'text', data: { text } }];
}

/** 消息段数组全是文本段时拼回一段文字；有别的段（图片、@……）就换不回去，返回 null */
export function segmentsToText(v: unknown): string | null {
    if (!Array.isArray(v)) return null;
    let out = '';
    for (const seg of v) {
        if (!isRecord(seg) || seg.type !== 'text' || !isRecord(seg.data) || typeof seg.data.text !== 'string') return null;
        out += seg.data.text;
    }
    return out;
}

// ---------------------------------------------------------------------------
// 时间戳
// ---------------------------------------------------------------------------

const pad = (n: number) => String(n).padStart(2, '0');

/** Unix 秒 → `datetime-local` 要的本地时间文本（精确到秒）；不是有效时间给空串 */
export function secondsToLocalInput(seconds: unknown): string {
    const n = typeof seconds === 'string' && /^\d+$/.test(seconds.trim()) ? Number(seconds) : seconds;
    if (typeof n !== 'number' || !Number.isFinite(n) || n < 0) return '';
    const d = new Date(n * 1000);
    if (Number.isNaN(d.getTime())) return '';
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** `datetime-local` 的文本 → Unix 秒；空的或认不出是 null */
export function localInputToSeconds(text: string): number | null {
    if (!text) return null;
    const ms = new Date(text).getTime();
    return Number.isFinite(ms) ? Math.floor(ms / 1000) : null;
}

// ---------------------------------------------------------------------------
// 发送
// ---------------------------------------------------------------------------

export interface SendState {
    hasTarget: boolean;
    running: boolean;
    action: string;
    stream: boolean;
    /** 参数里带本机文件占位（分块上传或文件参数选了本机文件） */
    localFileCount: number;
    parseOk: boolean;
    specLoading: boolean;
    channels: DebugChannels | undefined;
    /** 这个标签实际走的通道选择：标签自己指定的，否则跟顶栏 */
    channel: DebugChannelId;
}

/** 发送按钮为什么点不了；能发返回 null。顺序按「用户最先该解决的」排 */
export function sendBlocker(s: SendState): string | null {
    if (!s.hasTarget) return '先在顶栏选一个 Bot';
    if (!s.action.trim()) return '先填接口名';
    if (!s.running) return 'Bot 没在运行';
    if (s.stream) {
        // 分块传输只走内部通道；点名的 HTTP / WS 通道在前端就能确定不行
        const blocked = streamChannelBlocker(s.action, s.localFileCount, s.channel);
        if (blocked) return blocked;
    }
    if (!s.parseOk) return 'JSON 有错，改好再发';
    if (s.specLoading) return '正在读取接口说明…';
    if (s.channels) {
        if (s.channel.kind === 'auto' && s.channels.auto_call === null) return '没有能用的调用通道';
        if (s.channel.kind !== 'auto' && !findChannel(s.channels, s.channel)) return '选的通道已经不在了，换一条';
    }
    return null;
}

/** 等待秒数：一位小数，满 60 秒后按分秒写 */
export function elapsedText(ms: number): string {
    const s = Math.max(0, ms) / 1000;
    if (s < 60) return `${s.toFixed(1)} 秒`;
    const m = Math.floor(s / 60);
    return `${m} 分 ${Math.floor(s % 60)} 秒`;
}
