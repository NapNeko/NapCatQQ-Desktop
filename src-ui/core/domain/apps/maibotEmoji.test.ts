import { describe, expect, it } from 'vitest';
import { emojiMoves, normalizeEmojiTags, splitEmojiTags } from './maibotEmoji';

describe('emoji tags', () => {
    it('splits on the same separators as upstream', () => {
        expect(splitEmojiTags('开心，得意、 嘿嘿;无语；摸鱼\t晚安')).toEqual([
            '开心',
            '得意',
            '嘿嘿',
            '无语',
            '摸鱼',
            '晚安',
        ]);
        expect(splitEmojiTags(' ,，  ')).toEqual([]);
    });

    it('normalizes a list: re-split, drop blanks, dedupe in order', () => {
        expect(normalizeEmojiTags(['开心 得意', '开心', '  ', '无语'])).toEqual([
            '开心',
            '得意',
            '无语',
        ]);
    });
});

describe('emojiMoves', () => {
    it('offers only what makes sense for each status', () => {
        expect(emojiMoves('adopted')).toEqual(['unadopt', 'discard']);
        expect(emojiMoves('unknown')).toEqual(['adopt', 'discard']);
        expect(emojiMoves('discarded')).toEqual(['restore']);
    });
});
