import { afterEach, describe, expect, it, vi } from 'vitest';
import { newRequestId } from './ids';

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe('newRequestId', () => {
    afterEach(() => {
        vi.unstubAllGlobals();
    });

    it('有 randomUUID 时直接用它', () => {
        vi.stubGlobal('crypto', { randomUUID: () => '11111111-2222-4333-8444-555555555555' });
        expect(newRequestId()).toBe('11111111-2222-4333-8444-555555555555');
    });

    it('没有 randomUUID 但有 getRandomValues 时手拼合法的 v4', () => {
        vi.stubGlobal('crypto', {
            getRandomValues: (a: Uint8Array) => {
                a.fill(0xff);
                return a;
            },
        });
        const id = newRequestId();
        expect(id).toMatch(UUID_V4);
    });

    it('什么随机源都没有也能给出格式正确、互不相同的 id', () => {
        vi.stubGlobal('crypto', undefined);
        const a = newRequestId();
        const b = newRequestId();
        expect(a).toMatch(UUID_V4);
        expect(b).toMatch(UUID_V4);
        expect(a).not.toBe(b);
    });
});
