// 右栏里几份模块级小缓存（图片显示宽度、拉失败过的图 / 头像）的容器：有上限，失败记录会过期。
//
// 为什么要上限：调试台开一天，刷过去几万张图，Map 只增不减就是在漏内存。
// 为什么失败要过期：网络抖一下、代理刚好断了，不该让这张图 / 这个头像这次运行里永远显示「加载失败」。

/** 最近最少用的先丢；get 会把条目挪到最新 */
export class LruCache<V> {
    private readonly map = new Map<string, { value: V; bytes: number; expiresAt: number }>();
    private bytes = 0;

    constructor(
        private readonly max: number,
        private readonly budget?: {
            maxBytes: number;
            sizeOf: (value: V) => number;
            ttlMs: number;
        },
    ) {}

    private prune(now: number): void {
        if (!this.budget) return;
        for (const [key, entry] of this.map) {
            if (entry.expiresAt <= now) this.delete(key);
        }
    }

    get(key: string, now = Date.now()): V | undefined {
        this.prune(now);
        const entry = this.map.get(key);
        if (entry !== undefined) {
            this.map.delete(key);
            this.map.set(key, entry);
        }
        return entry?.value;
    }

    set(key: string, value: V, now = Date.now()): void {
        this.prune(now);
        this.delete(key);
        const bytes = this.budget ? Math.max(0, this.budget.sizeOf(value)) : 0;
        if (this.max <= 0 || !Number.isFinite(bytes) || bytes > (this.budget?.maxBytes ?? Infinity))
            return;
        this.map.set(key, {
            value,
            bytes,
            expiresAt: this.budget ? now + this.budget.ttlMs : Infinity,
        });
        this.bytes += bytes;
        while (this.map.size > this.max || this.bytes > (this.budget?.maxBytes ?? Infinity)) {
            const oldest = this.map.keys().next();
            if (oldest.done) break;
            this.delete(oldest.value);
        }
    }

    get size(): number {
        this.prune(Date.now());
        return this.map.size;
    }

    delete(key: string): void {
        this.bytes -= this.map.get(key)?.bytes ?? 0;
        this.map.delete(key);
    }
}

/** 记下「这个坏过」，过了 ttl 就当没坏过再试一次；同样有上限 */
export class ExpiringSet {
    private readonly map = new Map<string, number>();

    constructor(
        private readonly max: number,
        private readonly ttlMs: number,
    ) {}

    has(key: string, now: number = Date.now()): boolean {
        const at = this.map.get(key);
        if (at === undefined) return false;
        if (now - at > this.ttlMs) {
            this.map.delete(key);
            return false;
        }
        return true;
    }

    add(key: string, now: number = Date.now()): void {
        this.map.delete(key);
        this.map.set(key, now);
        if (this.map.size > this.max) {
            const oldest = this.map.keys().next();
            if (!oldest.done) this.map.delete(oldest.value);
        }
    }

    get size(): number {
        return this.map.size;
    }
}

/**
 * 缓存用的图片键。网址直接用；data: URL 可能几百 KB，不拿整串当键，
 * 改成「长度 + 抽样哈希」（隔一段取一个字符做 FNV-1a），足够区分不同的图，又不用扫完整串。
 */
export function imageCacheKey(url: string): string {
    if (!url.startsWith('data:')) return url;
    let h = 2166136261;
    const step = Math.max(1, Math.floor(url.length / 512));
    for (let i = 0; i < url.length; i += step) {
        h ^= url.charCodeAt(i);
        h = Math.imul(h, 16777619);
    }
    // 结尾那一段最能区分同一个模板生成的图，整段都算进去
    for (let i = Math.max(0, url.length - 64); i < url.length; i += 1) {
        h ^= url.charCodeAt(i);
        h = Math.imul(h, 16777619);
    }
    return `data#${url.length}#${(h >>> 0).toString(36)}`;
}

export const CACHE_MAX = 500;
export const FAILURE_TTL_MS = 5 * 60_000;
