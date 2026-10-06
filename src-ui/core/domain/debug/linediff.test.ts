import { describe, expect, it } from 'vitest';
import { diffAllSame, diffLines } from './linediff';

const kinds = (before: string, after: string) => diffLines(before, after).map((r) => r.kind);

describe('diffLines', () => {
    it('完全一样的文本：全是 same，左右都带着行', () => {
        const rows = diffLines('a\nb', 'a\nb');
        expect(rows).toEqual([
            { kind: 'same', left: 'a', right: 'a' },
            { kind: 'same', left: 'b', right: 'b' },
        ]);
        expect(diffAllSame(rows)).toBe(true);
    });

    it('中间插了一行：左右夹着的行不动，插入行只出现在右侧', () => {
        expect(diffLines('a\nc', 'a\nb\nc')).toEqual([
            { kind: 'same', left: 'a', right: 'a' },
            { kind: 'add', left: null, right: 'b' },
            { kind: 'same', left: 'c', right: 'c' },
        ]);
    });

    it('中间删了一行：只在左侧出现', () => {
        expect(kinds('a\nb\nc', 'a\nc')).toEqual(['same', 'remove', 'same']);
    });

    it('改了一行 = 先删后增', () => {
        expect(diffLines('{\n  "a": 1\n}', '{\n  "a": 2\n}')).toEqual([
            { kind: 'same', left: '{', right: '{' },
            { kind: 'remove', left: '  "a": 1', right: null },
            { kind: 'add', left: null, right: '  "a": 2' },
            { kind: 'same', left: '}', right: '}' },
        ]);
    });

    it('一块连续改动：左边删完再排右边增的，不来回穿插', () => {
        const rows = diffLines('x\na1\na2\ny', 'x\nb1\ny');
        expect(rows.map((r) => r.kind)).toEqual(['same', 'remove', 'remove', 'add', 'same']);
        expect(rows[3]).toEqual({ kind: 'add', left: null, right: 'b1' });
    });

    it('空文本按 0 行算：一边空就是全增 / 全删，两边都空没有行', () => {
        expect(kinds('', 'a\nb')).toEqual(['add', 'add']);
        expect(kinds('a', '')).toEqual(['remove']);
        expect(diffLines('', '')).toEqual([]);
    });

    it('结尾多一个换行不算多一行', () => {
        expect(diffLines('a\n', 'a')).toEqual([{ kind: 'same', left: 'a', right: 'a' }]);
    });

    it('行数乘积超过上限时掐头去尾：共同头尾保留，中段整段算改', () => {
        // 500×500 = 25 万格，超过 20 万上限
        const head = ['{', '  "head": 1,'];
        const tail = ['  "tail": 9', '}'];
        const midA = Array.from({ length: 499 }, (_, i) => `  "a${i}": ${i},`);
        const midB = Array.from({ length: 499 }, (_, i) => `  "b${i}": ${i},`);
        const rows = diffLines(
            [...head, ...midA, ...tail].join('\n'),
            [...head, ...midB, ...tail].join('\n'),
        );

        expect(rows.slice(0, head.length).every((r) => r.kind === 'same')).toBe(true);
        expect(rows.slice(-tail.length).every((r) => r.kind === 'same')).toBe(true);
        const middle = rows.slice(head.length, -tail.length);
        expect(middle.filter((r) => r.kind === 'remove')).toHaveLength(midA.length);
        expect(middle.filter((r) => r.kind === 'add')).toHaveLength(midB.length);
    });

    it('大文本但行数乘积没超限时照样走 LCS（改动在中间也对得齐）', () => {
        const padA = Array.from({ length: 300 }, (_, i) => `same ${i}`);
        const rows = diffLines(
            [...padA, 'old', ...padA].join('\n'),
            [...padA, 'new', ...padA].join('\n'),
        );
        expect(rows.filter((r) => r.kind !== 'same')).toEqual([
            { kind: 'remove', left: 'old', right: null },
            { kind: 'add', left: null, right: 'new' },
        ]);
    });
});
