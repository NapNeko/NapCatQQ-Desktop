import { describe, expect, it, vi } from 'vitest';
import { createQQFaceAssetService } from './qq-face-assets.service';

function cacheStorage() {
    const saved = new Map<string, Response>();
    const store = {
        match: async (value: RequestInfo) =>
            saved.get(typeof value === 'string' ? value : (value as Request).url)?.clone(),
        put: async (value: string, response: Response) => {
            saved.set(value, response.clone());
        },
        keys: async () => [...saved.keys()].map((value) => new Request(value)),
        delete: async (value: RequestInfo) =>
            saved.delete(typeof value === 'string' ? value : (value as Request).url),
    };
    return { saved, storage: { open: async () => store } as unknown as CacheStorage };
}
describe('persistent QQ face resources', () => {
    it('shares live resources and reloads persisted bytes after all blob leases are released', async () => {
        const source = 'https://koishi.js.org/QFace/assets/qq_emoji/364/apng/364.png';
        const { saved, storage } = cacheStorage();
        const fetcher = vi
            .fn()
            .mockImplementation(
                async () => new Response('image', { headers: { 'Content-Type': 'image/png' } }),
            );
        const urls = { create: vi.fn(() => 'blob:qq-face'), revoke: vi.fn() };
        const service = createQQFaceAssetService(fetcher, () => storage, urls);
        const [one, two] = await Promise.all([service.acquire(source), service.acquire(source)]);
        expect(fetcher).toHaveBeenCalledOnce();
        expect(one.url).toBe('blob:qq-face');
        one.release();
        expect(urls.revoke).not.toHaveBeenCalled();
        two.release();
        expect(urls.revoke).toHaveBeenCalledOnce();
        await vi.waitFor(() => expect(saved.has(source)).toBe(true));
        const offline = vi.fn().mockRejectedValue(new Error('offline'));
        const restored = await createQQFaceAssetService(offline, () => storage, urls).acquire(
            source,
        );
        expect(restored.url).toBe('blob:qq-face');
        expect(offline).not.toHaveBeenCalled();
        restored.release();
    });
    it('falls back to remote images when caching is unavailable and refuses an oversized streamed body', async () => {
        const source = 'https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png';
        const urls = { create: vi.fn(() => 'blob:bad'), revoke: vi.fn() };
        const fetcher = vi.fn().mockResolvedValue(
            new Response(new Uint8Array(2 * 1024 * 1024 + 1), {
                headers: { 'Content-Type': 'image/png' },
            }),
        );
        const lease = await createQQFaceAssetService(fetcher, () => undefined, urls).acquire(
            source,
        );
        expect(lease.url).toBe(source);
        expect(urls.create).not.toHaveBeenCalled();
        lease.release();
    });
    it('rejects timeouts so the renderer can try its mirror and clears the failed pending request', async () => {
        const source = 'https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png';
        const fetcher = vi.fn().mockRejectedValue(new DOMException('timeout', 'AbortError'));
        const service = createQQFaceAssetService(fetcher, () => undefined);
        await expect(service.acquire(source)).rejects.toThrow('超时');
        await expect(service.acquire(source)).rejects.toThrow('超时');
        expect(fetcher).toHaveBeenCalledTimes(2);
    });
    it('downloads visible resources with at most four simultaneous network reads', async () => {
        const finish: Array<() => void> = [];
        const fetcher = vi.fn(
            () =>
                new Promise<Response>((resolve) =>
                    finish.push(() =>
                        resolve(
                            new Response('image', { headers: { 'Content-Type': 'image/png' } }),
                        ),
                    ),
                ),
        );
        const service = createQQFaceAssetService(fetcher, () => undefined, {
            create: () => 'blob:face',
            revoke: vi.fn(),
        });
        const leases = Array.from({ length: 8 }, (_, index) =>
            service.acquire(
                'https://koishi.js.org/QFace/assets/qq_emoji/' + index + '/png/' + index + '.png',
            ),
        );
        await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(4));
        finish.splice(0).forEach((resolve) => resolve());
        await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(8));
        finish.splice(0).forEach((resolve) => resolve());
        (await Promise.all(leases)).forEach((lease) => lease.release());
    });
});
