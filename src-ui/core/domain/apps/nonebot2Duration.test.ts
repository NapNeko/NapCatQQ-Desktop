import { describe, expect, it } from 'vitest';
import { normalizeNoneBot2Duration } from './nonebot2Duration';

describe('Pydantic session duration', () => {
    it.each([
        ['120', 'PT120S'],
        ['-0.000001', '-PT0.000001S'],
        ['+1.25', 'PT1.25S'],
        ['0.0000009', 'PT0S'],
        ['PT0.0000009S', 'PT0.0000009S'],
        ['00:00:00.0000009', 'PT0S'],
        ['00:02:00', 'PT120S'],
        ['1 day, 02:03:04.000005', 'P1DT7384.000005S'],
        ['-79015.999995', '-PT79015.999995S'],
        ['1 02:03:04', 'P1DT7384S'],
        ['P1DT2H', 'P1DT2H'],
        ['+PT0.000001S', '+PT0.000001S'],
        ['-PT1.25S', '-PT1.25S'],
        ['P1W', 'P1W'],
        ['P1.5D', 'P1.5D'],
        ['PT1H', 'PT1H'],
        ['PT0.0000005S', 'PT0.0000005S'],
        ['-PT0.0000005S', '-PT0.0000005S'],
    ])('normalizes %s without losing its sign or microseconds', (raw, expected) => {
        expect(normalizeNoneBot2Duration(raw)).toBe(expected);
    });

    it.each([
        '',
        'abc',
        'NaN',
        'Infinity',
        'P',
        'PT',
        'P1DT',
        'P-1D',
        'P1D2D',
        'PT1S2M',
        'PT1.5H30M',
        '00:60:00',
        '00:00:99',
        '120\n',
        'P1000000000D',
        '-1 day, 02:03:04.000005',
        '-00:02:00',
    ])('rejects invalid, incomplete or overflowing duration %s', (raw) => {
        expect(normalizeNoneBot2Duration(raw)).toBeNull();
    });
});
