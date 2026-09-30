import { describe, expect, it } from 'vitest';
import { ExpiringSet, LruCache, imageCacheKey } from './boundedCache';

describe('LruCache', () => {
    it('超过上限丢最久没用的；读一次算用过', () => {
        const c = new LruCache<number>(3);
        c.set('a', 1);
        c.set('b', 2);
        c.set('c', 3);
        expect(c.get('a')).toBe(1); // a 变成最新
        c.set('d', 4); // 丢 b
        expect(c.get('b')).toBeUndefined();
        expect(c.get('a')).toBe(1);
        expect(c.get('c')).toBe(3);
        expect(c.get('d')).toBe(4);
        expect(c.size).toBe(3);
    });
});

describe('ExpiringSet', () => {
    it('失败记录过了 ttl 就不算了；也有上限', () => {
        const s = new ExpiringSet(2, 1000);
        s.add('x', 0);
        expect(s.has('x', 500)).toBe(true);
        expect(s.has('x', 1001)).toBe(false);
        // 过期的顺手删掉，再查也是没有
        expect(s.has('x', 0)).toBe(false);

        s.add('a', 0);
        s.add('b', 0);
        s.add('c', 0);
        expect(s.size).toBe(2);
        expect(s.has('a', 0)).toBe(false);
        expect(s.has('c', 0)).toBe(true);
    });
});

describe('imageCacheKey', () => {
    it('网址原样；data: URL 换成短键，不同的图键不同，同一张图键相同', () => {
        expect(imageCacheKey('https://example.com/a.jpg')).toBe('https://example.com/a.jpg');
        const big = `data:image/png;base64,${'A'.repeat(200_000)}B`;
        const other = `data:image/png;base64,${'A'.repeat(200_000)}C`;
        const k = imageCacheKey(big);
        expect(k.length).toBeLessThan(40);
        expect(imageCacheKey(big)).toBe(k);
        expect(imageCacheKey(other)).not.toBe(k);
    });
});
