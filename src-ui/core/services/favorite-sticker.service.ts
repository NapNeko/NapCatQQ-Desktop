// NapCat 收藏使用 Bot 主机缓存路径，SnowLuma 接受图片来源。
import { accountKey, record, text } from '../domain/chat/model';
import { debugErrorCopy } from '../domain/debug/errorCopy';
import type { Segment } from '../domain/debug/segments';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import { chatService } from './chat.service';
import {
    chatMediaService,
    createChatMediaService,
    type ImageSourceContext,
} from './chat-media.service';
import { isInlineImageReference } from './inline-image.service';

export class FavoriteStickerError extends Error {
    constructor(
        message: string,
        readonly uncertain = false,
    ) {
        super(message);
        this.name = 'FavoriteStickerError';
    }
}

function responseData(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') {
        const copy = debugErrorCopy(response.result.error);
        throw new Error([copy.title, copy.detail].filter(Boolean).join('：'));
    }
    const result = response.result.outcome;
    if (!result.ok) throw new Error(result.wording || result.message || '添加表情失败');
    if (result.truncated) throw new Error('收藏接口返回不完整，请在 QQ 检查结果');
    return result.data;
}
function sourceOf(segment: Segment): string {
    return (
        [segment.data.local_file, segment.data.url, segment.data.file]
            .map(text)
            .find((value) => /^(https?:\/\/|base64:\/\/|ncd-local-file:\/\/)/i.test(value)) ?? ''
    );
}
function hostPath(value: unknown): string {
    const path = text(value);
    return /^(?:[a-z]:[\\/]|\/|\\\\)/i.test(path) ? path : '';
}
export function createFavoriteStickerService(
    call: typeof chatService.call = (...args) => chatService.call(...args),
) {
    const pending = new Set<string>();
    const media = createChatMediaService(call);
    const listeners = new Map<string, Set<() => void>>();
    const scopeOf = (target: DebugTarget) => accountKey(target.bot_id, String(target.qq_id));
    async function napcatFile(target: DebugTarget, segment: Segment): Promise<string> {
        const source =
            [text(segment.data.url), text(segment.data.file), sourceOf(segment)].find((value) =>
                /^https?:\/\//i.test(value),
            ) || sourceOf(segment);
        if (source.startsWith('ncd-local-file://')) return source;
        const identifiers = [
            ...new Set(
                [text(segment.data.file_id), text(segment.data.file)].filter(
                    (value) =>
                        !!value &&
                        value !== '0' &&
                        !/^(?:https?:\/\/|base64:\/\/|ncd-local-file:\/\/)/i.test(value),
                ),
            ),
        ];
        for (const file of identifiers) {
            try {
                const result = record(
                    responseData(await call(target.bot_id, 'get_image', { file })),
                );
                const path = hostPath(result.file);
                if (path) return path;
            } catch {
                // 缓存失效时仍可使用这条消息的图片地址。
            }
        }
        if (!/^https?:\/\//i.test(source)) throw new Error('原图片已不可用，请刷新图片后再添加');
        const params = { url: source, thread_count: 1 };
        const result = record(responseData(await call(target.bot_id, 'download_file', params)));
        const path = hostPath(result.file);
        if (!path) throw new Error('协议未返回可收藏的图片文件');
        return path;
    }
    return {
        subscribe(target: DebugTarget, listener: () => void) {
            const scope = scopeOf(target);
            const group = listeners.get(scope) ?? new Set<() => void>();
            listeners.set(scope, group);
            group.add(listener);
            return () => {
                group.delete(listener);
                if (!group.size) listeners.delete(scope);
            };
        },
        async add(
            target: DebugTarget,
            segment: Segment,
            context: ImageSourceContext = {},
        ): Promise<void> {
            if (segment.type !== 'image' && segment.type !== 'mface')
                throw new Error('这条消息没有可收藏的图片');
            if (!target.running || target.online === false)
                throw new Error('账号未连接，请连接后添加表情');
            const scope = scopeOf(target);
            if (pending.has(scope)) throw new Error('正在添加表情，请稍后再试');
            pending.add(scope);
            try {
                if (
                    [
                        segment.data.inline_ref,
                        segment.data.local_file,
                        segment.data.file,
                        segment.data.url,
                        segment.data.base64,
                    ].some(isInlineImageReference)
                )
                    segment = {
                        ...segment,
                        data: await media.resolveImageSource(target, segment.data, {
                            ...context,
                            preferProtocol: target.backend === 'napcat',
                        }),
                    };
                const file =
                    target.backend === 'napcat'
                        ? await napcatFile(target, segment)
                        : sourceOf(segment);
                if (!file) throw new Error('原图片已不可用，请刷新图片后再添加');
                const params: Record<string, unknown> = { file };
                if (target.backend === 'napcat') {
                    const emoji = segment.data.emoji_id;
                    const pack = segment.data.emoji_package_id ?? segment.data.package_id;
                    const hasEmoji =
                        typeof emoji === 'number'
                            ? Number.isFinite(emoji)
                            : typeof emoji === 'string' && !!emoji.trim();
                    const hasPack =
                        typeof pack === 'number'
                            ? Number.isFinite(pack)
                            : typeof pack === 'string' && !!pack.trim();
                    if (hasEmoji && hasPack)
                        Object.assign(params, {
                            emoji_id: emoji,
                            package_id: pack,
                            is_mark_face: true,
                        });
                }
                let response: DebugCallResponse;
                try {
                    response = await call(target.bot_id, 'add_custom_face', params);
                } catch {
                    throw new FavoriteStickerError(
                        '添加结果待确认，请先在 QQ 收藏表情中查看',
                        true,
                    );
                }
                if (
                    response.result.kind === 'err' &&
                    ['timeout', 'transport', 'cancelled', 'internal'].includes(
                        response.result.error.kind,
                    )
                )
                    throw new FavoriteStickerError(
                        '添加结果待确认，请先在 QQ 收藏表情中查看',
                        true,
                    );
                if (
                    response.result.kind === 'ok' &&
                    response.result.outcome.ok &&
                    response.result.outcome.truncated
                )
                    throw new FavoriteStickerError(
                        '添加结果待确认，请先在 QQ 收藏表情中查看',
                        true,
                    );
                const result = record(responseData(response));
                if (
                    result.success === false ||
                    (typeof result.result === 'number' && result.result !== 0)
                )
                    throw new Error(
                        text(result.errMsg) || text(result.message) || 'QQ 未能添加这张表情',
                    );
                chatMediaService.invalidateFavorites(target);
                for (const listener of listeners.get(scope) ?? []) listener();
            } finally {
                pending.delete(scope);
            }
        },
    };
}
export const favoriteStickerService = createFavoriteStickerService();
