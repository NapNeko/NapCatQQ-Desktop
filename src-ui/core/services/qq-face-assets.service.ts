// 按需保存 QQ 表情图片；CacheStorage 在同源聊天窗口间共享。
const CACHE_NAME = 'ncd.qq-faces.v1';
const MAX_RESOURCE = 2 * 1024 * 1024;
const MAX_BYTES = 32 * 1024 * 1024;
const MAX_ENTRIES = 192;
interface SharedImage {
    url: string;
    refs: number;
}
export interface QQFaceImageLease {
    url: string;
    release: () => void;
}
async function imageBody(response: Response): Promise<Blob> {
    const type = (response.headers.get('Content-Type') ?? '').split(';')[0].trim();
    if (!/^image\/(png|apng|gif|webp)$/.test(type)) throw new Error('表情资源格式不可用');
    const reader = response.body?.getReader();
    if (!reader) {
        const blob = await response.blob();
        if (!blob.size || blob.size > MAX_RESOURCE) throw new Error('表情资源过大');
        return blob;
    }
    const chunks: BlobPart[] = [];
    let size = 0;
    try {
        for (;;) {
            const chunk = await reader.read();
            if (chunk.done) break;
            size += chunk.value.byteLength;
            if (size > MAX_RESOURCE) {
                await reader.cancel();
                throw new Error('表情资源过大');
            }
            chunks.push(chunk.value);
        }
    } finally {
        reader.releaseLock();
    }
    if (!size) throw new Error('表情资源为空');
    return new Blob(chunks, { type });
}

export function createQQFaceAssetService(
    fetcher: typeof fetch = (...args) => fetch(...args),
    storage: () => CacheStorage | undefined = () => globalThis.caches,
    objectUrl = {
        create: (blob: Blob) => URL.createObjectURL(blob),
        revoke: (url: string) => URL.revokeObjectURL(url),
    },
) {
    const active = new Map<string, SharedImage>();
    const pending = new Map<string, Promise<SharedImage>>();
    let writes = Promise.resolve();
    let downloading = 0;
    const waiting: Array<() => void> = [];
    async function downloadSlot<T>(work: () => Promise<T>): Promise<T> {
        if (downloading >= 4) {
            if (waiting.length >= 256) throw new Error('表情读取繁忙');
            await new Promise<void>((resolve) => waiting.push(resolve));
        } else downloading++;
        try {
            return await work();
        } finally {
            const next = waiting.shift();
            if (next) next();
            else downloading--;
        }
    }
    async function cache(): Promise<Cache | undefined> {
        try {
            return await storage()?.open(CACHE_NAME);
        } catch {
            return undefined;
        }
    }
    function save(store: Cache, source: string, blob: Blob) {
        writes = writes
            .catch(() => {})
            .then(async () => {
                await store.put(
                    source,
                    new Response(blob, {
                        headers: { 'Content-Type': blob.type, 'X-Ncd-Bytes': String(blob.size) },
                    }),
                );
                const keys = await store.keys();
                let bytes = 0;
                const sizes = await Promise.all(
                    keys.map(async (key) =>
                        Number(
                            (await store.match(key))?.headers.get('X-Ncd-Bytes') || MAX_RESOURCE,
                        ),
                    ),
                );
                sizes.forEach((size) => {
                    bytes += size;
                });
                for (
                    let i = 0;
                    i < keys.length && (keys.length - i > MAX_ENTRIES || bytes > MAX_BYTES);
                    i++
                ) {
                    await store.delete(keys[i]);
                    bytes -= sizes[i];
                }
            })
            .catch(() => {});
    }
    async function load(source: string): Promise<SharedImage> {
        const store = await cache();
        try {
            const saved = await store?.match(source);
            if (saved) {
                const blob = await saved.blob();
                if (blob.size > 0 && blob.size <= MAX_RESOURCE)
                    return { url: objectUrl.create(blob), refs: 0 };
                await store?.delete(source);
            }
        } catch {
            /* WebView 未开放缓存或对象 URL 时保留远程图片。 */
        }
        return downloadSlot(async () => {
            const controller = new AbortController();
            const timer = setTimeout(() => controller.abort(), 5_000);
            try {
                const response = await fetcher(source, {
                    signal: controller.signal,
                    credentials: 'omit',
                    referrerPolicy: 'no-referrer',
                });
                if (!response.ok || Number(response.headers.get('Content-Length')) > MAX_RESOURCE)
                    throw new Error('表情资源不可用');
                const blob = await imageBody(response);
                const url = objectUrl.create(blob);
                if (store) save(store, source, blob);
                return { url, refs: 0 };
            } catch (error) {
                if (
                    controller.signal.aborted ||
                    (typeof error === 'object' &&
                        error !== null &&
                        'name' in error &&
                        error.name === 'AbortError')
                )
                    throw new DOMException('表情资源读取超时', 'AbortError');
                return { url: source, refs: 0 };
            } finally {
                clearTimeout(timer);
            }
        });
    }
    return {
        async acquire(source: string): Promise<QQFaceImageLease> {
            let shared = active.get(source);
            if (!shared) {
                let job = pending.get(source);
                if (!job) {
                    job = load(source).then((value) => {
                        active.set(source, value);
                        return value;
                    });
                    pending.set(source, job);
                    void job.then(
                        () => {
                            pending.delete(source);
                        },
                        () => {
                            pending.delete(source);
                        },
                    );
                }
                shared = await job;
            }
            shared.refs++;
            const image = shared;
            let released = false;
            return {
                url: image.url,
                release: () => {
                    if (released) return;
                    released = true;
                    if (--image.refs === 0) {
                        active.delete(source);
                        if (image.url !== source) objectUrl.revoke(image.url);
                    }
                },
            };
        },
        async invalidate(source: string) {
            await (await cache())?.delete(source);
        },
    };
}
export const qqFaceAssetService = createQQFaceAssetService();
