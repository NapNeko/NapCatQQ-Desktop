// 消息搜索的纯逻辑：合并去重、匹配过滤、分类判定、日期分组与高亮切分，无 React / IPC 依赖。
import { markdownContent } from '../debug/markdown';
import { messagePreview } from '../debug/segments';
import type { Contact, Message } from './model';

export const RESULTS_PER_PAGE = 50;

export const categories = [
    ['all', '全部'],
    ['media', '图片/视频'],
    ['emoji', '表情'],
    ['file', '文件'],
    ['link', '链接'],
] as const;
export type Category = (typeof categories)[number][0];

export function inCategory(message: Message, category: Category) {
    if (category === 'all') return true;
    return message.segments.some((segment) => {
        const sticker =
            segment.type === 'mface' ||
            segment.type === 'face' ||
            (segment.type === 'image' &&
                Number(segment.data.sub_type ?? segment.data.subType) === 1);
        if (category === 'emoji') return sticker;
        if (category === 'media')
            return segment.type === 'video' || (segment.type === 'image' && !sticker);
        if (category === 'file') return segment.type === 'file';
        if (segment.type === 'markdown') return /https?:\/\//i.test(markdownContent(segment.data));
        return (
            ['text', 'markdown', 'json', 'xml'].includes(segment.type) &&
            Object.values(segment.data).some(
                (value) => typeof value === 'string' && /https?:\/\//i.test(value),
            )
        );
    });
}

export function dateGroup(at: number) {
    const date = new Date(at);
    return `${date.getFullYear()}/${String(date.getMonth() + 1).padStart(2, '0')}/${String(date.getDate()).padStart(2, '0')}`;
}

export function mergeSearchMessages(archived: readonly Message[], loaded: readonly Message[]) {
    const byKey = new Map<string, Message>();
    const byId = new Map<string, string>();
    for (const source of [archived, loaded]) {
        for (const message of source) {
            const key = `${message.session}/${message.key}`;
            const identity = message.id ? `${message.session}/${message.id}` : undefined;
            const previousKey = identity ? byId.get(identity) : undefined;
            const previous = byKey.get(previousKey ?? key);
            if (previousKey && previousKey !== key) byKey.delete(previousKey);
            // 已撤回记录不能被迟到的旧历史复活。
            byKey.set(
                key,
                previous?.recalled
                    ? {
                          ...message,
                          recalled: true,
                          segments: previous.segments.length ? previous.segments : message.segments,
                      }
                    : message,
            );
            if (identity) byId.set(identity, key);
        }
    }
    return [...byKey.values()].sort((a, b) => a.at - b.at);
}

export function buildSearchPattern(term: string): RegExp | null {
    return term ? new RegExp(term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'iu') : null;
}

export interface SearchableMessage {
    message: Message;
    preview: string;
    conversation: string;
}

export function buildSearchableMessages(
    messages: readonly Message[],
    conversations?: Readonly<Record<string, Contact>>,
): SearchableMessage[] {
    return messages.map((message) => ({
        message,
        preview: messagePreview(message.segments),
        conversation:
            conversations?.[message.session]?.name ||
            `${message.session.startsWith('group:') ? '群聊' : '私聊'} ${message.session.slice(message.session.indexOf(':') + 1)}`,
    }));
}

export function countConversations(searchable: readonly SearchableMessage[]) {
    return new Set(searchable.map(({ message }) => message.session)).size;
}

export function collectSenders(searchable: readonly SearchableMessage[]) {
    return [
        ...new Map(
            searchable.map(({ message }) => [
                message.senderId,
                message.mine ? '我' : message.senderName,
            ]),
        ).entries(),
    ];
}

export function filterMatches(
    searchable: readonly SearchableMessage[],
    filters: {
        pattern: RegExp | null;
        category: Category;
        sender: string;
        from: string;
        to: string;
    },
): SearchableMessage[] {
    const { pattern, category, sender, from, to } = filters;
    const startAt = from ? new Date(`${from}T00:00:00`).getTime() : -Infinity;
    const endAt = to ? new Date(`${to}T23:59:59.999`).getTime() + 1 : Infinity;
    return searchable
        .filter(
            ({ message, preview }) =>
                (!pattern || pattern.test(preview)) &&
                inCategory(message, category) &&
                (!sender || message.senderId === sender) &&
                message.at >= startAt &&
                message.at < endAt,
        )
        .reverse();
}

export interface HighlightPart {
    text: string;
    start: number;
    hit: boolean;
}

export function splitHighlight(text: string, pattern: RegExp): HighlightPart[] {
    const parts: HighlightPart[] = [];
    let offset = 0;
    for (const match of text.matchAll(new RegExp(pattern.source, 'giu'))) {
        const start = match.index;
        parts.push({ text: text.slice(offset, start), start: offset, hit: false });
        parts.push({ text: match[0], start, hit: true });
        offset = start + match[0].length;
    }
    parts.push({ text: text.slice(offset), start: offset, hit: false });
    return parts;
}
