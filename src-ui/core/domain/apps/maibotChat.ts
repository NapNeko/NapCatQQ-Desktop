// 麦麦试聊的协议层：上游统一 WebSocket 的帧 → 页面用的消息和事件，外加发出去的帧。
// 上游帧是它自己的 JSON、不是桌面端 IPC，类型写在这里不走 ts-rs；只取页面用得上的字段，
// 认不出的帧给 null，调用方忽略即可。

export type MaiBotChatSegment =
    | { type: 'text'; text: string }
    | { type: 'image'; src: string }
    | { type: 'emoji'; src: string }
    | { type: 'voice'; src: string }
    | { type: 'at'; name: string }
    | { type: 'reply'; sender: string; text: string }
    | { type: 'file'; name: string }
    | { type: 'forward'; count: number }
    | { type: 'other'; text: string };

export type MaiBotChatMessage = {
    /** 上游机器人回复不带 id，这里给空串，由存消息的一方补 */
    id: string;
    fromBot: boolean;
    senderName: string;
    segments: MaiBotChatSegment[];
    /** 秒 */
    at: number;
};

export type MaiBotChatEvent =
    | { kind: 'session'; botName: string; botQq?: string; userName: string }
    | { kind: 'history'; messages: MaiBotChatMessage[] }
    | { kind: 'message'; message: MaiBotChatMessage }
    | { kind: 'typing'; typing: boolean }
    | { kind: 'notice'; text: string; at: number; error: boolean }
    | { kind: 'nickname'; userName: string };

export type MaiBotChatFrame =
    | { op: 'ready' }
    | { op: 'response'; id: string; ok: boolean; error?: string }
    | { op: 'pong' }
    | { op: 'chat'; session: string; event: MaiBotChatEvent };

/** 发出去的图：上游认 `{name, mime_type, base64}`，base64 不带 data: 前缀 */
export type MaiBotChatImage = { name: string; mimeType: string; base64: string };

/** 上游一条消息最多收 8 张图，多的直接丢 */
export const CHAT_IMAGE_MAX = 8;

type Json = Record<string, unknown>;

const obj = (v: unknown): Json | undefined =>
    v !== null && typeof v === 'object' && !Array.isArray(v) ? (v as Json) : undefined;
const str = (v: unknown): string => (typeof v === 'string' ? v : '');
const num = (v: unknown): number | undefined => (typeof v === 'number' && Number.isFinite(v) ? v : undefined);
const nowSecs = () => Date.now() / 1000;

function textSegments(text: string): MaiBotChatSegment[] {
    return text ? [{ type: 'text', text }] : [];
}

function segmentOf(raw: unknown): MaiBotChatSegment | null {
    const s = obj(raw);
    if (!s) return null;
    const data = s.data;
    const d = obj(data);
    switch (s.type) {
        case 'text':
            return str(data) ? { type: 'text', text: str(data) } : null;
        case 'image':
        case 'emoji':
        case 'voice':
            // 上游读不到图时退成 text 段；这里只收真的 data URL
            return str(data).startsWith('data:') ? { type: s.type, src: str(data) } : null;
        case 'at':
            return {
                type: 'at',
                name: str(d?.target_user_cardname) || str(d?.target_user_nickname) || str(d?.target_user_id),
            };
        case 'reply':
            return {
                type: 'reply',
                sender: str(d?.target_message_sender_cardname) || str(d?.target_message_sender_nickname),
                text: str(d?.target_message_content),
            };
        case 'file':
            return { type: 'file', name: str(d?.name) || '文件' };
        case 'forward':
            return { type: 'forward', count: Array.isArray(data) ? data.length : 0 };
        default:
            return str(data) ? { type: 'other', text: str(data) } : null;
    }
}

function segmentsOf(raw: unknown, fallbackText: string): MaiBotChatSegment[] {
    const segs = Array.isArray(raw) ? raw.map(segmentOf).filter((s): s is MaiBotChatSegment => s !== null) : [];
    return segs.length > 0 ? segs : textSegments(fallbackText);
}

function attachments(raw: unknown, type: 'image' | 'emoji'): MaiBotChatSegment[] {
    if (!Array.isArray(raw)) return [];
    return raw.flatMap((it) => {
        const a = obj(it);
        const b64 = str(a?.base64);
        return b64 ? [{ type, src: `data:${str(a?.mime_type) || 'image/png'};base64,${b64}` }] : [];
    });
}

/** 打开会话时补的历史记录里的一条 */
function historyMessage(raw: unknown): MaiBotChatMessage | null {
    const m = obj(raw);
    if (!m) return null;
    return {
        id: str(m.id),
        fromBot: m.is_bot === true || m.type === 'bot',
        senderName: str(m.sender_name),
        // 纯文字的上游不给 segments，只给 content
        segments: segmentsOf(m.segments, str(m.content)),
        at: num(m.timestamp) ?? nowSecs(),
    };
}

function chatEvent(d: Json): MaiBotChatEvent | null {
    switch (d.type) {
        case 'session_info':
            return {
                kind: 'session',
                botName: str(d.bot_name) || '麦麦',
                botQq: str(d.bot_qq) || undefined,
                userName: str(d.user_name),
            };
        case 'history':
            return {
                kind: 'history',
                messages: (Array.isArray(d.messages) ? d.messages : [])
                    .map(historyMessage)
                    .filter((m): m is MaiBotChatMessage => m !== null),
            };
        case 'user_message': {
            // 回显给自己的那条：content 是「文字 + [图片]」这种展示串，原文在 raw_content
            const text = str(d.raw_content);
            const pics = [...attachments(d.images, 'image'), ...attachments(d.emojis, 'emoji')];
            return {
                kind: 'message',
                message: {
                    id: str(d.message_id),
                    fromBot: false,
                    senderName: str(obj(d.sender)?.name),
                    segments: text || pics.length > 0 ? [...textSegments(text), ...pics] : textSegments(str(d.content)),
                    at: num(d.timestamp) ?? nowSecs(),
                },
            };
        }
        case 'bot_message':
            return {
                kind: 'message',
                message: {
                    id: '',
                    fromBot: true,
                    senderName: str(obj(d.sender)?.name),
                    segments: d.message_type === 'rich' ? segmentsOf(d.segments, str(d.content)) : textSegments(str(d.content)),
                    at: num(d.timestamp) ?? nowSecs(),
                },
            };
        case 'typing':
            return { kind: 'typing', typing: d.is_typing === true };
        case 'system':
        case 'error':
            return str(d.content)
                ? { kind: 'notice', text: str(d.content), at: num(d.timestamp) ?? nowSecs(), error: d.type === 'error' }
                : null;
        case 'nickname_updated':
            return str(d.user_name) ? { kind: 'nickname', userName: str(d.user_name) } : null;
        default:
            return null;
    }
}

export function parseChatFrame(raw: string): MaiBotChatFrame | null {
    let v: unknown;
    try {
        v = JSON.parse(raw);
    } catch {
        return null;
    }
    const f = obj(v);
    if (!f) return null;
    switch (f.op) {
        case 'pong':
            return { op: 'pong' };
        case 'response': {
            const err = obj(f.error);
            return { op: 'response', id: str(f.id), ok: f.ok === true, error: err ? str(err.message) || str(err.code) : undefined };
        }
        case 'event': {
            if (f.domain === 'system') return f.event === 'ready' ? { op: 'ready' } : null;
            const d = obj(f.data);
            const event = f.domain === 'chat' && d ? chatEvent(d) : null;
            return event ? { op: 'chat', session: str(f.session), event } : null;
        }
        default:
            return null;
    }
}

const call = (id: string, session: string, method: string, data?: Json) =>
    JSON.stringify({ op: 'call', id, domain: 'chat', method, session, data: data ?? {} });

export const chatFrames = {
    ping: () => JSON.stringify({ op: 'ping' }),
    /** 桌面端报成 launcher（上游认的原生客户端形态）；restore 时上游不再发欢迎语 */
    open: (id: string, session: string, userId: string, userName: string, restore: boolean, version: string) =>
        call(id, session, 'session.open', {
            user_id: userId,
            user_name: userName,
            client: { type: 'launcher', name: 'NapCatQQ Desktop', version },
            restore,
        }),
    send: (id: string, session: string, userName: string, text: string, images: readonly MaiBotChatImage[]) =>
        call(id, session, 'message.send', {
            content: text,
            user_name: userName,
            images: images.slice(0, CHAT_IMAGE_MAX).map((i) => ({ name: i.name, mime_type: i.mimeType, base64: i.base64 })),
        }),
    rename: (id: string, session: string, userName: string) => call(id, session, 'session.update_nickname', { user_name: userName }),
    close: (id: string, session: string) => call(id, session, 'session.close'),
};

/** `data:<mime>;base64,<...>` 拆开；不是这种形状给 null */
export function splitDataUrl(url: string): { mimeType: string; base64: string } | null {
    const m = /^data:([^;,]+);base64,(.+)$/s.exec(url);
    return m ? { mimeType: m[1], base64: m[2] } : null;
}

// 隔这么久以上中间插一行时间；同一个人连着说、间隔不到这么久的不重复出名字
const TIME_GAP_SECS = 5 * 60;
const GROUP_GAP_SECS = 3 * 60;

/** 一条消息前要不要插时间、要不要出发言人 */
export function timelineMarks(messages: readonly Pick<MaiBotChatMessage, 'fromBot' | 'at'>[]) {
    return messages.map((m, i) => {
        const prev = i > 0 ? messages[i - 1] : undefined;
        const showTime = !prev || m.at - prev.at > TIME_GAP_SECS;
        return { showTime, showSender: showTime || !prev || prev.fromBot !== m.fromBot || m.at - prev.at > GROUP_GAP_SECS };
    });
}

/** 今天只给时分，今年给月日，更早带年 */
export function chatTimeLabel(secs: number, now = new Date()): string {
    const d = new Date(secs * 1000);
    const hm = d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
    if (d.toDateString() === now.toDateString()) return hm;
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (d.toDateString() === yesterday.toDateString()) return `昨天 ${hm}`;
    const md = `${d.getMonth() + 1}月${d.getDate()}日`;
    return d.getFullYear() === now.getFullYear() ? `${md} ${hm}` : `${d.getFullYear()}年${md} ${hm}`;
}
