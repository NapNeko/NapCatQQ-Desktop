// 聊天输入框的数据模型：文本框里是用户看得见的字（@ 进来的人显示成「@名字 」），
// 旁边另记一份「插进来过的 @」，发送时按这份把文字里的「@名字」换回 at 段。
//
// 用户把「@名字」删掉了，那个 at 就不发；自己手打的「@张三」不在名单里，照普通文字发。

import type { Segment } from './segments';

export interface Mention {
    /** QQ 号，或 'all'（全体成员） */
    qq: string;
    /** 文本框里显示的样子，含开头的 @，不含后面的空格 */
    label: string;
}

export interface ComposerDraft {
    text: string;
    mentions: Mention[];
}

export const EMPTY_DRAFT: ComposerDraft = { text: '', mentions: [] };

/**
 * 文本 + @ 名单 → 消息段。reply 放最前（OneBot 要求回复段在开头），
 * 然后按文字顺序交替 text / at。同一个位置能匹配多个 @ 时取最长的（「@张三丰」优先于「@张三」）。
 */
export function buildMessageSegments(text: string, mentions: readonly Mention[], replyTo?: number | null): Segment[] {
    const out: Segment[] = [];
    if (replyTo !== undefined && replyTo !== null) out.push({ type: 'reply', data: { id: String(replyTo) } });

    const body = text.replace(/\s+$/, '');
    const labels = [...new Map(mentions.map((m) => [m.label, m])).values()].sort((a, b) => b.label.length - a.label.length);
    let pos = 0;
    let buffer = '';
    const flush = () => {
        if (buffer !== '') out.push({ type: 'text', data: { text: buffer } });
        buffer = '';
    };
    while (pos < body.length) {
        let hit: Mention | undefined;
        if (body[pos] === '@') hit = labels.find((m) => body.startsWith(m.label, pos));
        if (hit) {
            flush();
            out.push({ type: 'at', data: { qq: hit.qq } });
            pos += hit.label.length;
        } else {
            buffer += body[pos];
            pos += 1;
        }
    }
    flush();
    return out;
}

/**
 * 插进文本框的「@名字」。同名的人会撞：群里有两个同名的，或者文本里已经 @ 过另一个同名的，
 * 就在后面带上 QQ 号，发送时每个 @ 都对得上人。@全体成员不会撞。
 */
export function mentionLabel(name: string, qq: string, existing: readonly Mention[], sameNameInGroup: number): string {
    const plain = `@${name}`;
    if (qq === 'all') return plain;
    const clash = sameNameInGroup > 1 || existing.some((m) => m.label === plain && m.qq !== qq);
    return clash ? `@${name}(${qq})` : plain;
}

/** 有没有能发的内容：光有一个回复段、或者只有空白不算 */
export function hasContent(segments: readonly Segment[]): boolean {
    return segments.some(
        (s) => s.type === 'at' || (s.type === 'text' && typeof s.data.text === 'string' && s.data.text.trim() !== ''),
    );
}

/** 只留下文本里还出现着的 @；删掉的人不该一直记着 */
export function pruneMentions(text: string, mentions: readonly Mention[]): Mention[] {
    const kept = mentions.filter((m) => text.includes(m.label));
    return kept.length === mentions.length ? (mentions as Mention[]) : kept;
}

/** 光标前面正在输入的「@xxx」：返回 @ 的位置和 @ 后面已经打了的字；不在输入 @ 时返回 null */
export function mentionQueryAt(text: string, caret: number): { start: number; query: string } | null {
    const before = text.slice(0, caret);
    const m = /(?:^|[^\w@])@([^\s@]{0,24})$/u.exec(before);
    if (!m) return null;
    const query = m[1] ?? '';
    return { start: caret - query.length - 1, query };
}

/** 发送成功后该留下什么：发送途中用户接着打的字不能被清掉 */
export function remainderAfterSend(current: string, sent: string): string {
    if (current === sent) return '';
    if (current.startsWith(sent)) return current.slice(sent.length).replace(/^\s+/, '');
    return current;
}
