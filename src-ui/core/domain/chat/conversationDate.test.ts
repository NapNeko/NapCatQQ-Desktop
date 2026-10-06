import { describe, expect, it } from 'vitest';
import { conversationDate } from './conversationDate';

describe('conversation date labels', () => {
    const now = new Date(2026, 9, 3, 12, 30);
    it('shows local time today and retains a full timestamp for the title and time element', () => {
        const date = new Date(2026, 9, 3, 9, 5, 7);
        expect(conversationDate(date.getTime(), now)).toEqual({
            label: '09:05',
            title: '2026/10/3 09:05:07',
            dateTime: date.toISOString(),
        });
    });
    it.each([
        [new Date(2026, 9, 2, 23, 59), '昨天'],
        [new Date(2026, 9, 1, 12, 30), '10/1'],
        [new Date(2025, 9, 3, 12, 30), '2025/10/3'],
    ])('labels earlier days without presenting them as today', (date, label) => {
        expect(conversationDate(date.getTime(), now)?.label).toBe(label);
    });
    it.each([
        [new Date(2027, 0, 1, 0, 1), new Date(2026, 11, 31, 23, 59)],
        [new Date(2028, 2, 1, 12), new Date(2028, 1, 29, 1)],
    ])('finds yesterday across year and leap-month boundaries', (today, yesterday) => {
        expect(conversationDate(yesterday.getTime(), today)?.label).toBe('昨天');
    });
    it.each([0, NaN, Infinity])('omits missing or invalid timestamps', (timestamp) => {
        expect(conversationDate(timestamp, now)).toBeNull();
    });
});
