// 麦麦表情包的标签和状态。标签就是上游的 description，拆法和上游一致（逗号、顿号、分号、空白），
// 后端 resources/emoji::normalize_tags 是同一套。

import type { MaiBotEmojiStatus } from '../../ipc/types';

const SEPARATORS = /[,，、;；\s]+/;

/** 一段输入拆成标签：「开心，得意 嘿嘿」→ 三个 */
export function splitEmojiTags(raw: string): string[] {
    return raw.split(SEPARATORS).map((t) => t.trim()).filter(Boolean);
}

/** 每一项都再拆一遍，去空、去重，保持先后 */
export function normalizeEmojiTags(tags: readonly string[]): string[] {
    const out: string[] = [];
    for (const t of tags.flatMap(splitEmojiTags)) if (!out.includes(t)) out.push(t);
    return out;
}

export const EMOJI_STATUS_LABEL: Readonly<Record<MaiBotEmojiStatus, string>> = {
    adopted: '收下',
    known: '认识',
    unknown: '不认识',
    discarded: '丢弃',
};

/** 这个状态下能做的操作，详情里按这个排按钮 */
export type EmojiMove = 'adopt' | 'unadopt' | 'discard' | 'restore';

export function emojiMoves(status: MaiBotEmojiStatus): EmojiMove[] {
    switch (status) {
        case 'adopted':
            return ['unadopt', 'discard'];
        case 'known':
        case 'unknown':
            return ['adopt', 'discard'];
        case 'discarded':
            return ['restore'];
    }
}
