import { describe, expect, it } from 'vitest';
import { formatRelativeTime, relativeTimeFromMs } from './relativeTime';

const NOW = Date.parse('2026-09-27T12:00:00Z');
const SEC = 1000;
const MIN = 60 * SEC;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe('relativeTimeFromMs', () => {
    it('walks up from seconds to years', () => {
        expect(relativeTimeFromMs(NOW - 2 * SEC, undefined, NOW)).toBe('刚刚');
        expect(relativeTimeFromMs(NOW - 30 * SEC, undefined, NOW)).toBe('30 秒前');
        expect(relativeTimeFromMs(NOW - 5 * MIN, undefined, NOW)).toBe('5 分钟前');
        expect(relativeTimeFromMs(NOW - 3 * HOUR, undefined, NOW)).toBe('3 小时前');
        expect(relativeTimeFromMs(NOW - 2 * DAY, undefined, NOW)).toBe('2 天前');
        expect(relativeTimeFromMs(NOW - 14 * DAY, undefined, NOW)).toBe('2 周前');
        expect(relativeTimeFromMs(NOW - 90 * DAY, undefined, NOW)).toBe('3 个月前');
        expect(relativeTimeFromMs(NOW - 800 * DAY, undefined, NOW)).toBe('2 年前');
    });

    it('treats future timestamps as just now', () => {
        expect(relativeTimeFromMs(NOW + 10 * MIN, undefined, NOW)).toBe('刚刚');
    });

    it('stops at maxDays', () => {
        expect(relativeTimeFromMs(NOW - 6 * DAY, 7, NOW)).toBe('6 天前');
        expect(relativeTimeFromMs(NOW - 7 * DAY, 7, NOW)).toBeNull();
    });
});

describe('formatRelativeTime', () => {
    it('returns null for unparsable input', () => {
        expect(formatRelativeTime('not a date')).toBeNull();
    });
});
