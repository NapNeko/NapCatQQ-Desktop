// 会话内图片字节独立计费；消息只携带短引用，引用不跨账号或进程恢复。
import { INLINE_IMAGE_PREFIX } from '../domain/chat/imageSource';
export { INLINE_IMAGE_PREFIX, isInlineImageReference } from '../domain/chat/imageSource';
const MAX_IMAGE_BYTES = 16 * 1024 * 1024;
interface Entry {
    scope: string;
    bytes: Uint8Array;
    cost: number;
    protected: boolean;
    url?: string;
}
export function createInlineImageService(
    options: { accountBytes?: number; totalBytes?: number; entries?: number } = {},
) {
    const accountBytes = options.accountBytes ?? 32 * 1024 * 1024;
    const totalBytes = options.totalBytes ?? 96 * 1024 * 1024;
    const entryLimit = options.entries ?? 256;
    const entries = new Map<string, Entry>();
    const scopeOf = (botId: string, selfId: string) => JSON.stringify([botId, selfId]);
    const drop = (reference: string) => {
        const entry = entries.get(reference);
        if (entry?.url) URL.revokeObjectURL(entry.url);
        entries.delete(reference);
    };
    const usage = (scope?: string) =>
        [...entries.values()].reduce(
            (sum, entry) => sum + (!scope || entry.scope === scope ? entry.cost : 0),
            0,
        );
    const find = (botId: string, selfId: string, reference: string) => {
        const entry = entries.get(reference);
        if (!entry || entry.scope !== scopeOf(botId, selfId)) return;
        entries.delete(reference);
        entries.set(reference, entry);
        return entry;
    };
    const encode = (bytes: Uint8Array) => {
        const chunks: string[] = [];
        for (let start = 0; start < bytes.length; start += 3 * 16384) {
            let chunk = '';
            const end = Math.min(bytes.length, start + 3 * 16384);
            for (let index = start; index < end; index++)
                chunk += String.fromCharCode(bytes[index]);
            chunks.push(btoa(chunk));
        }
        return chunks.join('');
    };
    const mime = (bytes: Uint8Array) =>
        bytes[0] === 255 && bytes[1] === 216
            ? 'image/jpeg'
            : bytes[0] === 71 && bytes[1] === 73 && bytes[2] === 70
              ? 'image/gif'
              : bytes[0] === 82 && bytes[1] === 73 && bytes[2] === 70
                ? 'image/webp'
                : 'image/png';
    return {
        capture(
            botId: string,
            selfId: string,
            source: string,
            onCreate?: (reference: string) => void,
        ): string {
            const raw = source
                .replace(/^base64:\/\//i, '')
                .replace(/^data:image\/[^;,]+;base64,/i, '');
            if (
                !raw ||
                raw.length > Math.ceil(MAX_IMAGE_BYTES / 3) * 4 ||
                !/^[A-Za-z0-9+/]+={0,2}$/.test(raw)
            )
                throw new Error('图片内容无效或超过 16 MiB');
            const decoded = atob(raw);
            if (decoded.length > MAX_IMAGE_BYTES) throw new Error('图片超过 16 MiB');
            // 包含字节数组与 Blob 的最坏复制量，显示图片不会绕过池预算。
            const cost = decoded.length * 2;
            const scope = scopeOf(botId, selfId);
            for (const [reference, entry] of entries) {
                if (entry.scope !== scope || entry.bytes.length !== decoded.length) continue;
                let equal = true;
                for (let index = 0; index < decoded.length; index++) {
                    if (entry.bytes[index] !== decoded.charCodeAt(index)) {
                        equal = false;
                        break;
                    }
                }
                if (equal) {
                    entry.protected = true;
                    find(botId, selfId, reference);
                    return reference;
                }
            }
            if (cost > accountBytes || cost > totalBytes) throw new Error('图片超过会话缓存容量');
            const fits = () =>
                usage(scope) + cost <= accountBytes &&
                usage() + cost <= totalBytes &&
                entries.size < entryLimit;
            for (const [reference, entry] of entries) {
                if (fits()) break;
                if (
                    !entry.protected &&
                    (entry.scope === scope ||
                        usage() + cost > totalBytes ||
                        entries.size >= entryLimit)
                )
                    drop(reference);
            }
            if (!fits()) throw new Error('图片缓存已满，请处理未完成的发送后重试');
            const bytes = new Uint8Array(decoded.length);
            for (let index = 0; index < decoded.length; index++)
                bytes[index] = decoded.charCodeAt(index);
            const reference = INLINE_IMAGE_PREFIX + crypto.randomUUID();
            entries.set(reference, { scope, bytes, cost, protected: true });
            onCreate?.(reference);
            return reference;
        },
        has(botId: string, selfId: string, reference: string): boolean {
            return entries.get(reference)?.scope === scopeOf(botId, selfId);
        },
        source(botId: string, selfId: string, reference: string): string | undefined {
            const entry = find(botId, selfId, reference);
            return entry ? 'base64://' + encode(entry.bytes) : undefined;
        },
        resolve(botId: string, selfId: string, reference: string): string | undefined {
            const entry = find(botId, selfId, reference);
            if (!entry) return;
            if (typeof URL.createObjectURL !== 'function')
                return `data:${mime(entry.bytes)};base64,${encode(entry.bytes)}`;
            entry.url ??= URL.createObjectURL(
                new Blob([entry.bytes.buffer as ArrayBuffer], { type: mime(entry.bytes) }),
            );
            return entry.url;
        },
        protect(botId: string, selfId: string, reference: string, protectedSource: boolean) {
            const entry = entries.get(reference);
            if (entry?.scope === scopeOf(botId, selfId)) entry.protected = protectedSource;
        },
        discard(botId: string, selfId: string, reference: string) {
            if (entries.get(reference)?.scope === scopeOf(botId, selfId)) drop(reference);
        },
        release(botId: string, selfId: string) {
            const scope = scopeOf(botId, selfId);
            for (const [reference, entry] of entries) if (entry.scope === scope) drop(reference);
        },
        stats() {
            return { entries: entries.size, bytes: usage() };
        },
    };
}
export const inlineImageService = createInlineImageService();
