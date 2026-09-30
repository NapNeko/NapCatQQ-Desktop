// OneBot 消息段：三种入参形态（数组 / 单个段 / CQ 码字符串）统一成数组，再给出一行文字预览。
// 事件里的 message、send_* 调用参数里的 message、raw_message 都走这里，聊天气泡和搜索共用同一份预览。

export interface Segment {
    type: string;
    data: Record<string, unknown>;
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** CQ 码转义只有这四种；一次扫描替换，避免 `&amp;#91;` 被连环反转义成 `[` */
const CQ_UNESCAPES: Record<string, string> = { '&amp;': '&', '&#91;': '[', '&#93;': ']', '&#44;': ',' };
const unescapeCq = (s: string) => (s.includes('&') ? s.replace(/&(?:amp|#91|#93|#44);/g, (m) => CQ_UNESCAPES[m] ?? m) : s);

/** 类型名里不会有逗号和方括号；值里的逗号一定已被转义成 &#44;，所以按逗号切参数是安全的 */
const CQ_CODE = /\[CQ:([^,[\]]+)((?:,[^[\]]*)?)\]/g;

/** 把 CQ 码文本拆成消息段；不成对的 `[CQ:` 当普通文字保留 */
export function parseCQ(text: string): Segment[] {
    const out: Segment[] = [];
    const pushText = (raw: string) => {
        if (raw !== '') out.push({ type: 'text', data: { text: unescapeCq(raw) } });
    };
    let last = 0;
    CQ_CODE.lastIndex = 0;
    for (let m = CQ_CODE.exec(text); m; m = CQ_CODE.exec(text)) {
        pushText(text.slice(last, m.index));
        const data: Record<string, unknown> = {};
        for (const pair of (m[2] ?? '').split(',')) {
            if (pair === '') continue;
            const eq = pair.indexOf('=');
            if (eq < 0) data[pair] = '';
            else data[pair.slice(0, eq)] = unescapeCq(pair.slice(eq + 1));
        }
        out.push({ type: (m[1] ?? '').trim(), data });
        last = m.index + m[0].length;
    }
    pushText(text.slice(last));
    return out;
}

function segmentOf(item: unknown): Segment | null {
    if (typeof item === 'string') return { type: 'text', data: { text: item } };
    if (!isRecord(item) || typeof item.type !== 'string') return null;
    return { type: item.type, data: isRecord(item.data) ? item.data : {} };
}

/** 消息统一成消息段数组：数组格式、单个段对象、CQ 码字符串都认，其它形态（含 null）给空数组 */
export function normalizeMessage(message: unknown): Segment[] {
    if (typeof message === 'string') return parseCQ(message);
    if (Array.isArray(message)) {
        const out: Segment[] = [];
        for (const item of message) {
            const seg = segmentOf(item);
            if (seg) out.push(seg);
        }
        return out;
    }
    const single = segmentOf(message);
    return single ? [single] : [];
}

const str = (v: unknown): string => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '');

/** 文件名之类可能是整段 URL 或 base64，预览里截一下免得撑爆一行 */
function clip(s: string, max = 40): string {
    return s.length > max ? `${s.slice(0, max)}…` : s;
}

/** 卡片消息的标题：json 卡片里翻 prompt / meta 里的 title，xml 卡片翻 brief / title 标签 */
function cardTitle(seg: Segment): string {
    const payload = seg.data.data;
    if (typeof payload !== 'string' || payload === '') return '';
    if (seg.type === 'json') {
        try {
            const obj: unknown = JSON.parse(payload);
            if (!isRecord(obj)) return '';
            if (typeof obj.prompt === 'string' && obj.prompt) return obj.prompt;
            const meta = obj.meta;
            if (isRecord(meta)) {
                for (const v of Object.values(meta)) {
                    if (isRecord(v)) {
                        const title = str(v.title) || str(v.desc);
                        if (title) return title;
                    }
                }
            }
            return typeof obj.desc === 'string' ? obj.desc : '';
        } catch {
            return '';
        }
    }
    const brief = /brief="([^"]*)"/.exec(payload) ?? /<title>([^<]*)<\/title>/.exec(payload);
    return brief?.[1] ?? '';
}

export function segmentPreview(seg: Segment): string {
    const d = seg.data;
    switch (seg.type) {
        case 'text':
            return str(d.text);
        case 'at': {
            const qq = str(d.qq);
            if (qq === 'all') return '@全体成员';
            return `@${str(d.name) || qq}`;
        }
        case 'image':
            return '[图片]';
        case 'face':
            return '[表情]';
        case 'reply':
            return '[回复]';
        case 'record':
            return '[语音]';
        case 'video':
            return '[视频]';
        case 'file': {
            const name = str(d.name) || str(d.file_name) || str(d.file);
            return name ? `[文件] ${clip(name)}` : '[文件]';
        }
        case 'forward':
            return '[聊天记录]';
        case 'json':
        case 'xml': {
            const title = cardTitle(seg);
            return title ? `[卡片] ${clip(title, 60)}` : '[卡片]';
        }
        case 'markdown':
            return '[Markdown]';
        case 'poke':
            return '[戳一戳]';
        case 'mface':
            return '[表情包]';
        default:
            return `[${seg.type}]`;
    }
}

/**
 * 预览按数组身份缓存：消息段数组建好后不会被改，而搜索框每敲一个字都要对几千条消息重算预览，
 * json 卡片还要 JSON.parse，不缓存会白白卡顿。
 */
const previewCache = new WeakMap<Segment[], string>();

export function messagePreview(segs: Segment[]): string {
    const cached = previewCache.get(segs);
    if (cached !== undefined) return cached;
    let out = '';
    for (const seg of segs) out += segmentPreview(seg);
    previewCache.set(segs, out);
    return out;
}
