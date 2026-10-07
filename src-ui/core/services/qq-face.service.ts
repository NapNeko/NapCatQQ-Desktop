// 图片资源目录不能代表账号的可发送目录。
import { QQ_FACE_FALLBACK, type QQSystemFace } from '../domain/chat/qqFaces';
import { record, text } from '../domain/chat/model';
import { chatService } from './chat.service';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';

const INDEX_URL = 'https://koishi.js.org/QFace/assets/qq_emoji/_index.v2.json';
const INDEX_MIRROR =
    'https://cdn.jsdelivr.net/gh/koishijs/QFace@master/public/assets/qq_emoji/_index.v2.json';
const CATALOG_STORAGE = 'ncd.qq-face-catalog.v1';
export const QQ_CLASSIC_FACES = QQ_FACE_FALLBACK.filter((face) => Number(face.id) < 260);
export interface QQFaceCatalog {
    faces: QQSystemFace[];
    limited: boolean;
}
const fallbackNames = new Map(QQ_FACE_FALLBACK.map((face) => [face.id, face.name]));
function storedFace(value: unknown): QQSystemFace | null {
    const row = record(value);
    if (
        typeof row.id !== 'string' ||
        !/^(0|[1-9]\d{0,5})$/.test(row.id) ||
        typeof row.name !== 'string'
    )
        return null;
    const face: QQSystemFace = { id: row.id, name: row.name.slice(0, 64) };
    if (Array.isArray(row.aliases))
        face.aliases = row.aliases
            .filter((word): word is string => typeof word === 'string')
            .slice(0, 16)
            .map((word) => word.slice(0, 64));
    if (typeof row.category === 'string') face.category = row.category.slice(0, 24);
    if (typeof row.super === 'boolean') face.super = row.super;
    if (typeof row.url === 'string' && /^https?:\/\//i.test(row.url))
        face.url = row.url.slice(0, 2_048);
    if (
        row.animationUrl ===
        `https://koishi.js.org/QFace/assets/qq_emoji/${row.id}/apng/${row.id}.png`
    )
        face.animationUrl = row.animationUrl as string;
    return face;
}
export function parseQQFaceCatalog(value: unknown): QQSystemFace[] {
    return Object.entries(record(record(value).emojis))
        .flatMap(([id, raw]) => {
            const row = record(raw);
            if (
                !/^\d{1,6}$/.test(id) ||
                row.emojiId !== id ||
                row.removed ||
                !Array.isArray(row.assets)
            )
                return [];
            // Unicode 表情与互动资源不能直接组成 OneBot face 段。
            if (
                !row.assets.some(
                    (asset) =>
                        record(asset).type === 'png' &&
                        record(asset).path === `assets/qq_emoji/${id}/png/${id}.png`,
                )
            )
                return [];
            const name =
                typeof row.describe === 'string'
                    ? row.describe.replace(/^\/+/, '').trim().slice(0, 64)
                    : '';
            const superFace = [1, 2, 3, 5].includes(Number(row.emojiType));
            const animationUrl = row.assets.some(
                (asset) =>
                    record(asset).type === 'apng' &&
                    record(asset).path === `assets/qq_emoji/${id}/apng/${id}.png`,
            )
                ? `https://koishi.js.org/QFace/assets/qq_emoji/${id}/apng/${id}.png`
                : undefined;
            const aliases = Array.isArray(row.associateWords)
                ? row.associateWords
                      .filter((word): word is string => typeof word === 'string')
                      .slice(0, 16)
                      .map((word) => word.slice(0, 64))
                : [];
            return [
                {
                    id,
                    name: name || `QQ 表情 ${id}`,
                    ...(aliases.length ? { aliases } : {}),
                    ...(typeof row.emojiType === 'number'
                        ? {
                              category: [2, 3, 5].includes(row.emojiType)
                                  ? '互动'
                                  : superFace
                                    ? '超级'
                                    : '经典',
                              super: superFace,
                          }
                        : {}),
                    ...(animationUrl ? { animationUrl } : {}),
                },
            ];
        })
        .sort((a, b) => Number(a.id) - Number(b.id));
}

export function parseAccountFaceCatalog(value: unknown): QQSystemFace[] {
    const packs = record(value).packs;
    if (!Array.isArray(packs) || packs.length > 128) throw new Error('表情目录不可用');
    const faces = new Map<string, QQSystemFace>();
    for (const pack of packs) {
        const emojis = record(pack).emojis;
        if (!Array.isArray(emojis) || emojis.length > 2_048) throw new Error('表情目录不可用');
        for (const raw of emojis) {
            const row = record(raw);
            const id = text(row.q_sid);
            if (!/^(0|[1-9]\d{0,5})$/.test(id)) continue;
            if (
                row.is_super &&
                ![row.ani_sticker_pack_id, row.ani_sticker_id, row.ani_sticker_type].every(
                    (value) =>
                        typeof value === 'number' && Number.isSafeInteger(value) && value >= 0,
                )
            )
                continue;
            const name =
                text(row.q_des).replace(/^\/+/, '').trim().slice(0, 64) ||
                fallbackNames.get(id) ||
                `QQ 表情 ${id}`;
            const aliases = Array.isArray(row.emoji_name_alias)
                ? row.emoji_name_alias
                      .filter((alias): alias is string => typeof alias === 'string')
                      .slice(0, 16)
                      .map((alias) => alias.slice(0, 64))
                : [];
            const url = /^https?:\/\//i.test(text(row.url)) ? text(row.url) : undefined;
            const category = text(record(pack).pack_name).trim().slice(0, 24);
            if (!faces.has(id))
                faces.set(id, {
                    id,
                    name,
                    aliases,
                    url,
                    ...(category ? { category } : {}),
                    ...(row.is_super === true ? { super: true } : {}),
                });
            if (faces.size > 2_048) throw new Error('表情目录过大');
        }
    }
    if (!faces.size) throw new Error('表情目录为空');
    return [...faces.values()].sort((a, b) => Number(a.id) - Number(b.id));
}

export function createQQFaceService(
    fetcher: typeof fetch = (...args) => fetch(...args),
    call: typeof chatService.call = (...args) => chatService.call(...args),
    store?: Pick<Storage, 'getItem' | 'setItem'>,
) {
    let cached = QQ_FACE_FALLBACK;
    let expires = 0;
    let pending: Promise<QQSystemFace[]> | undefined;
    try {
        const saved = store?.getItem(CATALOG_STORAGE);
        if (saved && saved.length <= 512 * 1024) {
            const value = record(JSON.parse(saved));
            if (
                Array.isArray(value.faces) &&
                value.faces.length > 0 &&
                value.faces.length <= 2_048
            ) {
                const faces = value.faces.flatMap((value) => {
                    const face = storedFace(value);
                    return face ? [face] : [];
                });
                if (faces.length === value.faces.length) {
                    cached = faces;
                    expires = Math.min(
                        Number(value.expires) || 0,
                        Date.now() + 24 * 60 * 60 * 1000,
                    );
                }
            }
        }
    } catch {
        /* 存储不可用时继续使用内置目录。 */
    }
    type AccountCache = { value: QQFaceCatalog; expires: number; pending?: Promise<QQFaceCatalog> };
    const accounts = new Map<string, AccountCache>();
    const accountKey = (target: DebugTarget) =>
        JSON.stringify([target.backend, target.bot_id, target.qq_id]);
    const service = {
        peekAccount(target: DebugTarget): QQFaceCatalog | undefined {
            return target.backend === 'snowluma'
                ? accounts.get(accountKey(target))?.value
                : { faces: cached, limited: false };
        },
        peek(id: string): QQSystemFace | undefined {
            return cached.find((face) => face.id === id);
        },
        catalog(refresh = false): Promise<QQSystemFace[]> {
            if (!refresh && Date.now() < expires) return Promise.resolve(cached);
            if (pending) return pending;
            pending = (async () => {
                try {
                    let faces: QQSystemFace[] = [];
                    for (const source of [INDEX_URL, INDEX_MIRROR]) {
                        const controller = new AbortController();
                        const timer = setTimeout(() => controller.abort(), 5_000);
                        try {
                            const response = await fetcher(source, {
                                signal: controller.signal,
                                credentials: 'omit',
                                referrerPolicy: 'no-referrer',
                            });
                            if (!response.ok) throw new Error('表情目录读取失败');
                            const body = await response.text();
                            if (body.length > 2 * 1024 * 1024) throw new Error('表情目录过大');
                            faces = parseQQFaceCatalog(JSON.parse(body));
                            if (!faces.length || faces.length > 2_048)
                                throw new Error('表情目录不可用');
                            break;
                        } catch {
                            faces = [];
                        } finally {
                            clearTimeout(timer);
                        }
                    }
                    if (!faces.length) throw new Error('表情目录读取失败');
                    cached = faces;
                    expires = Date.now() + 24 * 60 * 60 * 1000;
                    try {
                        const saved = JSON.stringify({ faces, expires });
                        if (saved.length <= 512 * 1024) store?.setItem(CATALOG_STORAGE, saved);
                    } catch {
                        /* 配额不足不影响表情使用。 */
                    }
                } catch {
                    expires = Date.now() + 60_000;
                } finally {
                    pending = undefined;
                }
                return cached;
            })();
            return pending;
        },
        async forAccount(
            target: DebugTarget,
            refresh = false,
            read = call,
        ): Promise<QQFaceCatalog> {
            if (target.backend !== 'snowluma')
                return { faces: await service.catalog(refresh), limited: false };
            const key = accountKey(target);
            let entry = accounts.get(key);
            if (entry?.pending) return entry.pending;
            if (entry && !refresh && Date.now() < entry.expires) return entry.value;
            if (!entry) {
                // 旧版本没有账号目录接口时，仅提供经典编号。
                entry = { value: { faces: QQ_CLASSIC_FACES, limited: true }, expires: 0 };
                if (accounts.size >= 32) {
                    const oldest = [...accounts].find(([, value]) => !value.pending)?.[0];
                    if (oldest) accounts.delete(oldest);
                }
                if (accounts.size < 32) accounts.set(key, entry);
            }
            const cache = entry;
            cache.pending = (async () => {
                try {
                    // 同时更新协议端发送时使用的目录，修复过期的超级表情映射。
                    const response = await read(target.bot_id, 'fetch_sys_faces', {
                        refresh: true,
                    });
                    if (
                        response.result.kind === 'err' ||
                        !response.result.outcome.ok ||
                        response.result.outcome.truncated
                    )
                        throw new Error('表情目录读取失败');
                    cache.value = {
                        faces: parseAccountFaceCatalog(response.result.outcome.data),
                        limited: false,
                    };
                    cache.expires = Date.now() + 10 * 60_000;
                } catch {
                    cache.expires = Date.now() + 60_000;
                } finally {
                    cache.pending = undefined;
                }
                return cache.value;
            })();
            return cache.pending;
        },
        async validate(target: DebugTarget, ids: string[], read = call): Promise<void> {
            if (target.backend !== 'snowluma' || !ids.length) return;
            const catalog = await service.forAccount(target, false, read);
            const supported = new Set(catalog.faces.map((face) => face.id));
            const missing = ids.find((id) => !supported.has(id));
            if (missing)
                throw new Error(`当前账号不支持 QQ 表情 ${missing}，请更新表情目录或重新选择`);
        },
        invalidate(target: DebugTarget) {
            const entry = accounts.get(accountKey(target));
            if (entry) entry.expires = 0;
        },
    };
    return service;
}
function localCatalogStore(): Storage | undefined {
    try {
        return globalThis.localStorage;
    } catch {
        return undefined;
    }
}
export const qqFaceService = createQQFaceService(undefined, undefined, localCatalogStore());
const RECENT_STORAGE = 'ncd.qq-face-recent.v1';
function recentFaces(): Record<string, string[]> {
    try {
        const saved = localCatalogStore()?.getItem(RECENT_STORAGE);
        if (!saved || saved.length > 64 * 1024) return {};
        const value = record(JSON.parse(saved));
        return Object.fromEntries(
            Object.entries(value)
                .slice(-32)
                .map(([identity, ids]) => [
                    identity,
                    Array.isArray(ids)
                        ? ids
                              .filter(
                                  (id): id is string =>
                                      typeof id === 'string' && /^(0|[1-9]\d{0,5})$/.test(id),
                              )
                              .slice(0, 24)
                        : [],
                ]),
        );
    } catch {
        return {};
    }
}
export function loadRecentQQFaces(identity: string): string[] {
    return recentFaces()[identity] ?? [];
}
export function rememberQQFace(identity: string, id: string): string[] {
    const all = recentFaces();
    const next = [id, ...(all[identity] ?? []).filter((value) => value !== id)].slice(0, 24);
    delete all[identity];
    const entries = Object.entries(all).slice(-31);
    try {
        localCatalogStore()?.setItem(
            RECENT_STORAGE,
            JSON.stringify(Object.fromEntries([...entries, [identity, next]])),
        );
    } catch {
        /* 最近使用不影响插入草稿。 */
    }
    return next;
}
