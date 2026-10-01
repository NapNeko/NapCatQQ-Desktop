// 完整消息构建器的数据模型：草稿 = 手打文字 + @ 名单 + 构建器段（rich），
// 输入框和构建器是同一份 ComposerEntry 的两种视图。
//
// 顺序规则：rich 永远排在手打文字前面（和 QQ 一样，附件在上、文字在后）。
// 打开构建器 = `[...rich, ...文字换成的段]` 整个给到段列表；写回时整份只有 text / at
// 才折叠回输入框，否则全留在 rich、输入框清空 —— 任何顺序组合都只用一份数据表达，不复读。

import { buildMessageSegments, mentionLabel, type ComposerDraft, type Mention } from './composerModel';
import type { Segment } from './segments';

export interface ComposerEntry extends ComposerDraft {
    /** 构建器管理的段，排在手打文字前面；没用过构建器是 [] */
    rich: Segment[];
}

export const EMPTY_ENTRY: ComposerEntry = { text: '', mentions: [], rich: [] };

const str = (v: unknown): string => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '');

/** 草稿 → 段列表：rich 在前，手打文字按 @ 名单切成末尾的 text / at */
export function parseEntryToSegments(entry: ComposerEntry): Segment[] {
    return [...entry.rich, ...buildMessageSegments(entry.text, entry.mentions)];
}

/**
 * 回复段归一：输入框上挂着的回复（点气泡来的）永远在最前；
 * 没有的话段里已有的第一个 reply 提到最前，多余 reply 段丢掉（OneBot 只认开头的回复），
 * 和 SegmentView「回复段不管排在哪都画在最上面」同一个口径。
 */
export function normalizeReplies(segments: readonly Segment[], chipReplyId?: number | null): Segment[] {
    const own = segments.find((s) => s.type === 'reply') ?? null;
    if (chipReplyId === undefined || chipReplyId === null) {
        if (!own) return segments as Segment[];
        return [own, ...segments.filter((s) => s.type !== 'reply')];
    }
    return [{ type: 'reply', data: { id: String(chipReplyId) } }, ...segments.filter((s) => s.type !== 'reply')];
}

/** 发送时用的最终消息段 */
export function assembleMessage(entry: ComposerEntry, chipReplyId?: number | null): Segment[] {
    return normalizeReplies(parseEntryToSegments(entry), chipReplyId);
}

/** 有没有能发的内容：光有一个回复段、或者只有空白文字不算；图片等非文本段本身就能发 */
export function hasMessageContent(segments: readonly Segment[]): boolean {
    return segments.some(
        (s) =>
            s.type !== 'reply' &&
            (s.type !== 'text' || (typeof s.data.text === 'string' && s.data.text.trim() !== '')),
    );
}

/**
 * 段列表换回输入框草稿：只有 text / at 才换得回去。
 * at 段的显示名优先用段里的 name，其次在 avoid（输入框现有 @ 名单）里找同 QQ 的旧名字复用，
 * 来回编辑不会把「@阿强」退化成「@10003」；都没有就显示 QQ 号，重名撞车按 mentionLabel 带 (QQ 号)。
 */
export function segmentsToDraft(segments: readonly Segment[], avoid: readonly Mention[] = []): ComposerDraft | null {
    if (segments.some((s) => s.type !== 'text' && s.type !== 'at')) return null;
    const displayName = (seg: Segment): string => {
        const qq = str(seg.data.qq);
        return str(seg.data.name) || (qq === 'all' ? '全体成员' : qq || '某人');
    };
    const nameCount = new Map<string, number>();
    for (const s of segments) {
        if (s.type !== 'at') continue;
        const name = displayName(s);
        nameCount.set(name, (nameCount.get(name) ?? 0) + 1);
    }
    let text = '';
    let lastWasAt = false;
    const mentions: Mention[] = [];
    const usedFromAvoid = new Set<number>();
    const append = (piece: string, isAt: boolean) => {
        text = text === '' ? piece : /\s$/.test(text) || /^\s/.test(piece) ? text + piece : `${text} ${piece}`;
        lastWasAt = isAt;
    };
    for (const seg of segments) {
        if (seg.type === 'text') {
            const t = str(seg.data.text);
            if (t !== '') append(t, false);
            continue;
        }
        const qq = str(seg.data.qq);
        let label: string | null = null;
        if (qq !== '') {
            const idx = avoid.findIndex((m, i) => !usedFromAvoid.has(i) && m.qq === qq);
            if (idx >= 0) {
                usedFromAvoid.add(idx);
                label = (avoid[idx] as Mention).label;
            }
        }
        if (label === null) {
            const name = displayName(seg);
            label = mentionLabel(name, qq, [...avoid, ...mentions], nameCount.get(name) ?? 1);
        }
        append(label, true);
        if (!mentions.some((m) => m.label === label && m.qq === qq)) mentions.push({ qq, label });
    }
    // 末尾是 @ 时补一个空格好接着打字，和输入框里插入 @ 的惯例一致
    return { text: lastWasAt ? `${text} ` : text, mentions };
}

/** 转回输入框时构建出的文字排在手打文字前面；两边相邻处没有空白才补一个空格 */
export function joinDraftText(built: string, typed: string): string {
    if (built === '') return typed;
    if (typed === '') return built;
    return /\s$/.test(built) || /^\s/.test(typed) ? built + typed : `${built} ${typed}`;
}

// ---------------------------------------------------------------------------
// 段类型表与起步数据
// ---------------------------------------------------------------------------

export interface BuilderKind {
    type: string;
    label: string;
}

/** 构建器能手动加的段类型 */
export const BUILDER_KINDS: readonly BuilderKind[] = [
    { type: 'text', label: '文本' },
    { type: 'at', label: '@ 成员' },
    { type: 'face', label: '表情' },
    { type: 'image', label: '图片' },
    { type: 'record', label: '语音' },
    { type: 'video', label: '视频' },
    { type: 'reply', label: '回复' },
    { type: 'poke', label: '戳一戳' },
    { type: 'markdown', label: 'Markdown' },
    { type: 'json', label: 'JSON 卡片' },
    { type: 'xml', label: 'XML 卡片' },
];

export function builderKindLabel(type: string): string {
    return BUILDER_KINDS.find((k) => k.type === type)?.label ?? type;
}

export function blankSegment(type: string): Segment {
    switch (type) {
        case 'text':
            return { type, data: { text: '' } };
        case 'at':
            return { type, data: { qq: '' } };
        case 'face':
        case 'reply':
            return { type, data: { id: '' } };
        case 'image':
        case 'record':
        case 'video':
            return { type, data: { file: '' } };
        case 'poke':
            return { type, data: { type: '', id: '' } };
        case 'markdown':
            return { type, data: { content: '' } };
        case 'json':
        case 'xml':
            return { type, data: { data: '' } };
        default:
            return { type, data: {} };
    }
}

/**
 * 段的小毛病写成一行提示；不挡「使用这些段」—— 调试台有时候会故意发畸形消息看上游反应，
 * 这里只负责让人看见问题。
 */
export function segmentIssue(seg: Segment): string | null {
    const d = seg.data;
    switch (seg.type) {
        case 'text':
            return str(d.text).trim() === '' ? '文字是空的' : null;
        case 'at': {
            const qq = str(d.qq);
            if (qq === '') return '还没填 QQ 号';
            return qq === 'all' || /^\d+$/.test(qq) ? null : 'QQ 号只能是数字，全体填 all';
        }
        case 'face':
            return /^\d+$/.test(str(d.id)) ? null : '表情 id 是数字';
        case 'image':
        case 'record':
        case 'video':
            return str(d.file) === '' ? '还没填地址（URL / base64 / 路径）' : null;
        case 'reply':
            return /^-?\d+$/.test(str(d.id)) ? null : '回复的消息 id 是数字';
        case 'poke':
            return /^\d+$/.test(str(d.type)) && /^\d+$/.test(str(d.id)) ? null : '类型和 id 都是数字';
        case 'markdown':
            return str(d.content).trim() === '' ? '内容是空的' : null;
        case 'json': {
            const payload = str(d.data);
            if (payload.trim() === '') return '卡片 JSON 是空的';
            try {
                JSON.parse(payload);
                return null;
            } catch {
                return '不是合法的 JSON';
            }
        }
        case 'xml':
            return str(d.data).trim() === '' ? '卡片 XML 是空的' : null;
        default:
            return null;
    }
}
