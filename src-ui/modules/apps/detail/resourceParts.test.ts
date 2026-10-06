import { describe, expect, it } from 'vitest';
import { pickSpan } from './resourceParts';

const order = [11, 12, 13, 14, 15];

describe('pickSpan', () => {
    it('plain click only touches the clicked row', () => {
        expect(pickSpan(new Set(), 11, 13, false, order)).toEqual({ keys: [13], on: true });
        expect(pickSpan(new Set([13]), 11, 13, false, order)).toEqual({ keys: [13], on: false });
    });

    it('shift click spans from the last clicked row, either direction', () => {
        expect(pickSpan(new Set([12]), 12, 14, true, order)).toEqual({
            keys: [12, 13, 14],
            on: true,
        });
        expect(pickSpan(new Set([14]), 14, 11, true, order)).toEqual({
            keys: [11, 12, 13, 14],
            on: true,
        });
    });

    it('shift click on a picked row clears the whole span', () => {
        expect(pickSpan(new Set([12, 13, 14]), 12, 14, true, order)).toEqual({
            keys: [12, 13, 14],
            on: false,
        });
    });

    it('falls back to a single row when there is no anchor on this page', () => {
        expect(pickSpan(new Set(), null, 13, true, order)).toEqual({ keys: [13], on: true });
        // 翻页后上一页那条已经不在 order 里
        expect(pickSpan(new Set(), 99, 13, true, order)).toEqual({ keys: [13], on: true });
    });
});
