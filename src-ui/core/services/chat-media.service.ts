// 只消费协议返回的媒体内容，不把主机文件路径交给 WebView。
import { chatService } from './chat.service';
import { id, record, text } from '../domain/chat/model';
import { normalizeMessage, type Segment } from '../domain/debug/segments';
import { isLocalFileToken, LOCAL_FILE_PREFIX } from '../domain/debug/streamActions';
import { debugErrorCopy } from '../domain/debug/errorCopy';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import { inlineImageService, isInlineImageReference } from './inline-image.service';

export type {
    ForwardNode,
    FavoriteEmoji,
    ImageReadOptions,
    ImageSourceContext,
} from '../domain/chat/media';
import type {
    FavoriteEmoji,
    ForwardNode,
    ImageReadOptions,
    ImageSourceContext,
} from '../domain/chat/media';
type ImageConsumer = { start?: () => void };
interface ImageJob {
    promise: Promise<string>;
    controller: AbortController;
    started: boolean;
    consumers: Set<ImageConsumer>;
}
const imageCancelled = () => new DOMException('图片读取已取消', 'AbortError');
function dataOf(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') {
        const copy = debugErrorCopy(response.result.error);
        throw new Error([copy.title, copy.detail].filter(Boolean).join('：'));
    }
    const result = response.result.outcome;
    if (!result.ok)
        throw new Error(result.wording || result.message || `请求失败（${result.retcode}）`);
    if (result.truncated) throw new Error('媒体内容过大，无法完整读取');
    return result.data;
}
function playableUrl(value: unknown): string {
    const candidate = text(value);
    return /^(https?:\/\/|data:(?:video|audio|image)\/|blob:)/i.test(candidate) ? candidate : '';
}
function outboundImage(data: Record<string, unknown>, file: string): Segment {
    const next: Record<string, unknown> = { ...data, file };
    // NapCat 优先 url；展示地址不能盖过已解析的 Bot 文件或新签名来源。
    delete next.url;
    delete next.path;
    delete next.local_file;
    delete next.source_message_id;
    delete next.source_image_index;
    delete next.inline_ref;
    delete next.base64;
    return { type: 'image', data: next };
}
function botFilePath(value: unknown): string {
    const path = text(value);
    return /^(?:[a-z]:[\\/]|\/|\\\\)/i.test(path) ? path : '';
}
function base64Payload(result: Record<string, unknown>): string {
    for (const [key, value] of Object.entries(result)) {
        const raw = text(value).replace(/\s/g, '');
        if (!raw) continue;
        if (key === 'base64') return raw.replace(/^base64:\/\//i, '');
        if (/^base64:\/\//i.test(raw)) return raw.slice('base64://'.length);
    }
    return '';
}
function imageBytes(result: Record<string, unknown>): string {
    const value = base64Payload(result);
    if (!value) return '';
    if (
        value.length > 16 * 1024 * 1024 ||
        value.length % 4 !== 0 ||
        !/^[A-Za-z0-9+/]+={0,2}$/.test(value)
    )
        throw new Error('图片内容不完整或过大');
    const mime = value.startsWith('/9j/')
        ? 'image/jpeg'
        : value.startsWith('R0lGOD')
          ? 'image/gif'
          : value.startsWith('UklGR')
            ? 'image/webp'
            : 'image/png';
    return `data:${mime};base64,${value}`;
}
function videoMime(name: string): string {
    const extension = name.toLowerCase().match(/\.([a-z0-9]+)(?:$|[?#])/i)?.[1];
    return extension === 'webm'
        ? 'video/webm'
        : extension === 'mov'
          ? 'video/quicktime'
          : extension === 'ogg' || extension === 'ogv'
            ? 'video/ogg'
            : extension === 'm3u8'
              ? 'application/vnd.apple.mpegurl'
              : 'video/mp4';
}
function forwardNodes(value: unknown): ForwardNode[] {
    if (!Array.isArray(value)) throw new Error('此协议未返回可识别的聊天记录');
    if (value.length > 500) throw new Error('转发消息过多，暂时无法展开');
    return value.map((raw) => {
        const row = record(raw);
        const data = row.type === 'node' ? record(row.data) : row;
        const sender = record(data.sender);
        const senderId = id(sender.user_id) || id(data.user_id) || id(data.uin);
        const content = data.message ?? data.content ?? data.raw_message;
        const messageId = id(data.message_id);
        // NapCat 内嵌转发有时返回 node 段，保持为可按需展开的安全消息段。
        const segments = normalizeMessage(content).map((segment) =>
            segment.type === 'node'
                ? { type: 'forward', data: { content: [segment] } }
                : messageId && (segment.type === 'image' || segment.type === 'mface')
                  ? { ...segment, data: { ...segment.data, source_message_id: messageId } }
                  : segment,
        );
        return {
            senderId,
            name:
                text(sender.card) ||
                text(sender.nickname) ||
                text(data.nickname) ||
                text(data.name) ||
                senderId ||
                '未知发送者',
            time: typeof data.time === 'number' ? data.time * 1000 : undefined,
            segments,
        };
    });
}
export function createChatMediaService(
    call: typeof chatService.call = (...args) => chatService.call(...args),
    readLocal: (path: string) => Promise<string> = (path) => chatService.readLocalImage(path),
) {
    const pendingImages = new Map<string, ImageJob>();
    const favoriteCache = new Map<string, { value: FavoriteEmoji[]; expires: number }>();
    const pendingFavorites = new Map<string, Promise<FavoriteEmoji[]>>();
    const favoriteKey = (target: DebugTarget) =>
        JSON.stringify([target.backend, target.bot_id, target.qq_id]);
    let activeImages = 0;
    const imageQueue: Array<{ start: () => void }> = [];
    const withImageSlot = async (
        work: () => Promise<string>,
        signal: AbortSignal,
        started: () => void,
    ): Promise<string> => {
        if (signal.aborted) throw imageCancelled();
        if (activeImages >= 4) {
            if (imageQueue.length >= 256) throw new Error('图片读取繁忙，请稍后重试');
            await new Promise<void>((resolve, reject) => {
                const cancelled = () => {
                    const index = imageQueue.indexOf(entry);
                    if (index >= 0) imageQueue.splice(index, 1);
                    reject(imageCancelled());
                };
                const entry = {
                    start: () => {
                        signal.removeEventListener('abort', cancelled);
                        resolve();
                    },
                };
                imageQueue.push(entry);
                signal.addEventListener('abort', cancelled, { once: true });
            });
        } else activeImages += 1;
        try {
            if (signal.aborted) throw imageCancelled();
            started();
            return await work();
        } finally {
            // 把槽位直接交给下一项，防止新请求抢走已排队请求的名额。
            const next = imageQueue.shift();
            if (next) next.start();
            else activeImages -= 1;
        }
    };
    const consumeImage = (job: ImageJob, options: ImageReadOptions): Promise<string> =>
        new Promise((resolve, reject) => {
            if (options.signal?.aborted) {
                reject(imageCancelled());
                return;
            }
            let timer: ReturnType<typeof setTimeout> | undefined;
            const consumer = {
                start: () => {
                    if (timer !== undefined) return;
                    options.onReadStart?.();
                    timer = setTimeout(() => {
                        cleanup();
                        if (!job.consumers.size) job.controller.abort();
                        reject(new Error('图片读取超时'));
                    }, 15_000);
                },
            };
            job.consumers.add(consumer);
            const cleanup = () => {
                clearTimeout(timer);
                job.consumers.delete(consumer);
                options.signal?.removeEventListener('abort', cancelled);
            };
            const cancelled = () => {
                cleanup();
                if (!job.consumers.size) job.controller.abort();
                reject(imageCancelled());
            };
            options.signal?.addEventListener('abort', cancelled, { once: true });
            if (job.started) consumer.start?.();
            job.promise.then(
                (value) => {
                    cleanup();
                    resolve(value);
                },
                (error) => {
                    cleanup();
                    reject(error);
                },
            );
        });
    // 自己刚发的图以本机字节为准；同步判源，无本地来源时不改变原有的调用时序。
    const localToken = (data: Record<string, unknown>): string =>
        [text(data.local_file), text(data.file)].find(
            (value) => value.startsWith('base64://') || isLocalFileToken(value),
        ) ?? '';
    const inlineReference = (data: Record<string, unknown>) =>
        [data.inline_ref, data.local_file, data.file, data.url, data.base64].find(
            isInlineImageReference,
        );
    const resolveImageSource = async (
        target: DebugTarget,
        data: Record<string, unknown>,
        context: ImageSourceContext = {},
    ): Promise<Record<string, unknown>> => {
        const reference = inlineReference(data);
        if (!reference) return data;
        const messageId = id(data.source_message_id) || id(context.messageId);
        const source =
            context.preferProtocol && messageId && !messageId.startsWith('local:')
                ? undefined
                : inlineImageService.source(target.bot_id, String(target.qq_id), reference);
        if (source) {
            const resolved: Record<string, unknown> = { ...data, file: source, local_file: source };
            delete resolved.inline_ref;
            if (isInlineImageReference(resolved.base64)) delete resolved.base64;
            if (isInlineImageReference(resolved.url)) delete resolved.url;
            return resolved;
        }
        if (!messageId || messageId.startsWith('local:'))
            throw new Error('原图片缓存已释放，请重新添加图片');
        const rawIndex = context.imageIndex ?? Number(data.source_image_index ?? 0);
        const imageIndex = Number.isInteger(rawIndex) && rawIndex >= 0 ? rawIndex : 0;
        const result = record(
            dataOf(await call(target.bot_id, 'get_msg', { message_id: messageId })),
        );
        const images = normalizeMessage(result.message ?? result.raw_message).filter(
            (segment) => segment.type === 'image' || segment.type === 'mface',
        );
        const recovered = images[imageIndex]?.data;
        if (!recovered || inlineReference(recovered)) throw new Error('原消息未返回可用的图片来源');
        return recovered;
    };
    const resolveImage = async (
        target: DebugTarget,
        data: Record<string, unknown>,
        refresh: boolean,
        signal?: AbortSignal,
    ): Promise<string> => {
        // 文件名可能在不同消息中复用；完整资源地址能避免 get_image 命中另一张同名图。
        const identifiers = [
            ...new Set(
                [text(data.url), text(data.file_id), text(data.file)].filter(
                    (value) => value && value !== '0' && !isInlineImageReference(value),
                ),
            ),
        ];
        if (!identifiers.length) throw new Error('这张图片缺少文件标识');
        let failure: unknown = new Error('协议未返回可播放图片地址');
        for (const file of identifiers) {
            try {
                if (signal?.aborted) throw imageCancelled();
                const params = file === text(data.file_id) ? { file_id: file } : { file };
                const result = record(dataOf(await call(target.bot_id, 'get_image', params)));
                if (signal?.aborted) throw imageCancelled();
                const bytes = imageBytes(result);
                if (bytes) return bytes;
                const url = playableUrl(result.url) || playableUrl(result.file);
                if (url) return url;
                const path = botFilePath(result.file);
                if (target.backend === 'napcat' && target.host.kind === 'local' && path) {
                    const local = imageBytes({ base64: await readLocal(path) });
                    if (local) return local;
                }
            } catch (error) {
                if (signal?.aborted) throw imageCancelled();
                failure = error;
            }
        }
        // 刷新不能回退到已知失效的签名地址。
        if (!refresh) {
            const direct = playableUrl(data.url) || playableUrl(data.file);
            if (direct) return direct;
        }
        throw failure;
    };
    const readFavorites = async (
        target: DebugTarget,
        action: 'fetch_custom_face' | 'fetch_custom_face_detail',
    ): Promise<FavoriteEmoji[]> => {
        for (let count = 256; count <= 65_536; count *= 2) {
            const result = dataOf(await call(target.bot_id, action, { count }));
            if (!Array.isArray(result)) throw new Error('此协议未返回收藏表情列表');
            if (result.length >= count) continue;
            const entries = new Map<string, FavoriteEmoji>();
            for (const value of result) {
                const row = record(value);
                const url = typeof value === 'string' ? value : text(row.url);
                if (!/^https?:\/\//i.test(url)) continue;
                if (!entries.has(url))
                    entries.set(url, { url, description: text(row.desc).trim().slice(0, 256) });
            }
            if (result.length && !entries.size) throw new Error('收藏表情地址不可用');
            const items = [...entries.values()];
            return target.backend === 'snowluma' ? items.reverse() : items;
        }
        throw new Error('收藏表情过多，无法完整读取');
    };
    return {
        resolveImageSource,
        isImageSourceAlive(target: DebugTarget, data: Record<string, unknown>): boolean {
            const reference = inlineReference(data);
            return (
                !reference || inlineImageService.has(target.bot_id, String(target.qq_id), reference)
            );
        },
        async imageForSend(
            target: DebugTarget,
            data: Record<string, unknown>,
            context: ImageSourceContext = {},
        ): Promise<Segment> {
            if (inlineReference(data)) data = await resolveImageSource(target, data, context);
            const emoji = id(data.emoji_id).trim();
            const pack = id(data.emoji_package_id ?? data.package_id ?? data.tab_id).trim();
            const packageId = Number(pack);
            const key = text(data.key).trim();
            if (emoji && /^\d+$/.test(pack) && Number.isSafeInteger(packageId) && key) {
                return {
                    type: 'mface',
                    data: {
                        emoji_id: emoji,
                        emoji_package_id: packageId,
                        key,
                        summary: text(data.summary) || text(data.name),
                    },
                };
            }
            const token = localToken(data);
            if (token) return outboundImage(data, token);
            const carriedBytes = base64Payload(data);
            if (carriedBytes) {
                imageBytes({ base64: carriedBytes });
                return outboundImage(data, `base64://${carriedBytes}`);
            }
            const identifiers = [
                ...new Set(
                    [text(data.file_id), text(data.file), text(data.url)].filter(
                        (value) => !!value && value !== '0' && !isInlineImageReference(value),
                    ),
                ),
            ];
            let failure: unknown = new Error('原图片已不可用，请重新添加后发送');
            for (const file of identifiers) {
                try {
                    const params = file === text(data.file_id) ? { file_id: file } : { file };
                    const result = record(dataOf(await call(target.bot_id, 'get_image', params)));
                    const path = botFilePath(result.file);
                    if (target.backend === 'napcat' && path) return outboundImage(data, path);
                    const bytes = base64Payload(result);
                    if (bytes) {
                        imageBytes({ base64: bytes });
                        return outboundImage(data, `base64://${bytes}`);
                    }
                    const url = playableUrl(result.url) || playableUrl(result.file);
                    if (/^https?:\/\//i.test(url)) return outboundImage(data, url);
                } catch (error) {
                    failure = error;
                }
            }
            // 自己添加的远端表情以 file 携带来源，不是接收消息里的展示 url。
            if (!text(data.url) && !text(data.file_id) && /^https?:\/\//i.test(text(data.file)))
                return outboundImage(data, text(data.file));
            throw failure;
        },
        async image(
            target: DebugTarget,
            data: Record<string, unknown>,
            refresh = false,
            options: ImageReadOptions = {},
        ): Promise<string> {
            if (options.signal?.aborted) throw imageCancelled();
            const reference = inlineReference(data);
            if (reference) {
                const preview = inlineImageService.resolve(
                    target.bot_id,
                    String(target.qq_id),
                    reference,
                );
                if (preview) return preview;
            }
            const token = localToken(data);
            const direct = token
                ? ''
                : playableUrl(data.url) || playableUrl(data.file) || imageBytes(data);
            if (!token && direct && !refresh) return direct;
            if (token.startsWith('base64://')) {
                try {
                    const bytes = imageBytes({ base64: token });
                    if (bytes) return bytes;
                } catch {
                    /* 无效本机字节仍可回退远端标识。 */
                }
            }
            const key = JSON.stringify([
                target.bot_id,
                target.qq_id,
                refresh,
                data.file_id,
                data.file,
                data.url,
                token.startsWith('base64://') ? '' : token,
                options.context?.messageId ?? data.source_message_id,
                options.context?.imageIndex ?? data.source_image_index,
            ]);
            const existing = pendingImages.get(key);
            if (existing && !existing.controller.signal.aborted)
                return consumeImage(existing, options);
            const job: ImageJob = {
                promise: undefined as unknown as Promise<string>,
                controller: new AbortController(),
                started: false,
                consumers: new Set(),
            };
            job.promise = withImageSlot(
                async () => {
                    if (reference) {
                        data = await resolveImageSource(target, data, options.context);
                        if (job.controller.signal.aborted) throw imageCancelled();
                    }
                    if (token && !token.startsWith('base64://')) {
                        try {
                            const bytes = imageBytes({
                                base64: await readLocal(token.slice(LOCAL_FILE_PREFIX.length)),
                            });
                            if (job.controller.signal.aborted) throw imageCancelled();
                            if (bytes) return bytes;
                        } catch {
                            if (job.controller.signal.aborted) throw imageCancelled();
                            /* 本机文件可能已被移动，回退协议链路。 */
                        }
                    }
                    if (!refresh) {
                        const fallback =
                            direct ||
                            playableUrl(data.url) ||
                            playableUrl(data.file) ||
                            imageBytes(data);
                        if (fallback) return fallback;
                    }
                    return resolveImage(target, data, refresh, job.controller.signal);
                },
                job.controller.signal,
                () => {
                    job.started = true;
                    for (const consumer of job.consumers) consumer.start?.();
                },
            );
            if (pendingImages.size < 256) pendingImages.set(key, job);
            const complete = () => {
                if (pendingImages.get(key) === job) pendingImages.delete(key);
            };
            void job.promise.then(complete, complete);
            return consumeImage(job, options);
        },
        async forward(target: DebugTarget, data: Record<string, unknown>): Promise<ForwardNode[]> {
            const inline = data.content ?? data.messages;
            if (Array.isArray(inline) && inline.length) return forwardNodes(inline);
            const resourceId =
                id(data.id) || id(data.res_id) || id(data.forward_id) || id(data.message_id);
            if (!resourceId) {
                if (Array.isArray(inline)) return [];
                throw new Error('这条聊天记录缺少可读取的标识');
            }
            const params =
                target.backend === 'snowluma' ? { id: resourceId } : { message_id: resourceId };
            return forwardNodes(
                record(dataOf(await call(target.bot_id, 'get_forward_msg', params))).messages,
            );
        },
        async record(target: DebugTarget, data: Record<string, unknown>): Promise<string> {
            const file = text(data.file) || text(data.file_id) || text(data.url);
            if (!file) throw new Error('这条语音缺少文件标识');
            const result = record(
                dataOf(await call(target.bot_id, 'get_record', { file, out_format: 'mp3' })),
            );
            const base64 = base64Payload(result);
            // SnowLuma 转码后仍带原 SILK 地址，必须先消费转码字节。
            if (base64) {
                if (base64.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(base64))
                    throw new Error('协议返回的音频内容不完整');
                if (base64.length > 16 * 1024 * 1024) throw new Error('媒体内容过大，无法播放');
                const mime =
                    result.out_format === 'wav'
                        ? 'audio/wav'
                        : result.out_format === 'ogg'
                          ? 'audio/ogg'
                          : 'audio/mpeg';
                return 'data:' + mime + ';base64,' + base64;
            }
            const url = playableUrl(result.url) || playableUrl(result.file);
            if (url && !/\.silk(?:$|[?#])/i.test(url)) return url;
            throw new Error('协议未返回可播放音频，请确认服务端支持 MP3 转码后重试');
        },
        async video(
            target: DebugTarget,
            data: Record<string, unknown>,
            refresh = false,
        ): Promise<string> {
            const direct =
                playableUrl(data.url) || playableUrl(data.file) || playableUrl(data.path);
            if (direct && !refresh) return direct;
            const identifiers = [
                ...new Set(
                    [text(data.file_id), text(data.file), text(data.path), text(data.url)].filter(
                        (value) => value && value !== '0',
                    ),
                ),
            ];
            if (!identifiers.length) throw new Error('这条视频缺少文件标识');
            let failure: unknown = new Error('协议未返回可播放视频地址');
            for (const file of identifiers) {
                try {
                    const params = file === text(data.file_id) ? { file_id: file } : { file };
                    const result = record(dataOf(await call(target.bot_id, 'get_file', params)));
                    const url = playableUrl(result.url) || playableUrl(result.file);
                    if (url) return url;
                    const base64 = base64Payload(result);
                    if (
                        base64 &&
                        base64.length % 4 === 0 &&
                        /^[A-Za-z0-9+/]+={0,2}$/.test(base64)
                    ) {
                        if (base64.length > 16 * 1024 * 1024)
                            throw new Error('视频内容过大，无法播放');
                        return `data:${videoMime(text(result.file_name) || file)};base64,${base64}`;
                    }
                } catch (error) {
                    failure = error;
                }
            }
            throw failure;
        },
        async transcript(target: DebugTarget, messageId: string): Promise<string> {
            if (!messageId.trim()) throw new Error('这条语音缺少消息标识');
            const result = record(
                dataOf(await call(target.bot_id, 'fetch_ptt_text', { message_id: messageId })),
            );
            const value = text(result.text).trim();
            if (!value) throw new Error('语音转文字没有返回内容');
            return value;
        },
        async favorites(target: DebugTarget): Promise<string[]> {
            // 两端接口均为前 N 项而非游标分页；扩大窗口直到返回不足一页。
            return (await readFavorites(target, 'fetch_custom_face')).map((item) => item.url);
        },
        favoriteDetails(target: DebugTarget, refresh = false): Promise<FavoriteEmoji[]> {
            const key = favoriteKey(target);
            const entry = favoriteCache.get(key);
            if (!refresh && entry && entry.expires > Date.now())
                return Promise.resolve(entry.value);
            const existing = pendingFavorites.get(key);
            if (existing) return existing;
            const job = (async () => {
                let value: FavoriteEmoji[];
                try {
                    value = await readFavorites(target, 'fetch_custom_face_detail');
                } catch {
                    value = await readFavorites(target, 'fetch_custom_face');
                }
                favoriteCache.delete(key);
                if (value.length <= 8_192) {
                    let retained = [...favoriteCache.values()].reduce(
                        (count, item) => count + item.value.length,
                        0,
                    );
                    for (const [oldKey, old] of favoriteCache) {
                        if (favoriteCache.size < 8 && retained + value.length <= 8_192) break;
                        favoriteCache.delete(oldKey);
                        retained -= old.value.length;
                    }
                    favoriteCache.set(key, { value, expires: Date.now() + 60_000 });
                }
                return value;
            })();
            pendingFavorites.set(key, job);
            void job.then(
                () => pendingFavorites.delete(key),
                () => pendingFavorites.delete(key),
            );
            return job;
        },
        invalidateFavorites(target: DebugTarget) {
            favoriteCache.delete(favoriteKey(target));
        },
    };
}
export const chatMediaService = createChatMediaService();
