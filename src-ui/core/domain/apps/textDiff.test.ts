import { describe, expect, it } from 'vitest';
import { changedLines, lineDiff } from './textDiff';

describe('lineDiff', () => {
    it('keeps shared lines and marks the rest', () => {
        const d = lineDiff('a\nb\nc', 'a\nB\nc\nd');
        expect(d).toEqual([
            { kind: 'same', text: 'a' },
            { kind: 'del', text: 'b' },
            { kind: 'add', text: 'B' },
            { kind: 'same', text: 'c' },
            { kind: 'add', text: 'd' },
        ]);
        expect(changedLines(d)).toBe(3);
    });

    it('treats identical text as no change', () => {
        expect(changedLines(lineDiff('x\ny', 'x\ny'))).toBe(0);
    });
});
