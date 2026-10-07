import { describe, expect, it } from 'vitest';
import { createInlineImageService } from './inline-image.service';

describe('inline image byte pool', () => {
    it('reuses same-account bytes after recovery without charging a second protected copy', () => {
        const pool = createInlineImageService({ accountBytes: 32, totalBytes: 64 });
        const source = 'base64://' + btoa('0123456789');
        const created: string[] = [];
        const first = pool.capture('bot', '99', source, (value) => created.push(value));
        expect(pool.capture('bot', '99', source, (value) => created.push(value))).toBe(first);
        expect(created).toEqual([first]);
        expect(pool.stats()).toEqual({ entries: 1, bytes: 20 });
        expect(pool.source('bot', '99', first)).toBe(source);
        expect(pool.has('bot', '100', first)).toBe(false);
        expect(pool.capture('bot', '100', source)).not.toBe(first);
        pool.release('bot', '99');
        expect(pool.has('bot', '99', first)).toBe(false);
    });
});
