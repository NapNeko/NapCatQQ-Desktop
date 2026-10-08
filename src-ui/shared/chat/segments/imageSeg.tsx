// 图片 / 表情包段：完整等比缩放、按地址缓存尺寸、失败可重试。
// 缓存放模块级，虚拟列表重新挂载时复用已知宽高与已解析地址。

import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Check, Copy, Image as ImageIcon, ImageOff } from 'lucide-react';
import { cn } from '../../utils/cn';
import { useChatView } from '../chatContext';
import { useCopy } from '../rightParts';
import { CACHE_MAX, LruCache, imageCacheKey } from '../boundedCache';
import type { ImageReadOptions, ImageSourceContext } from '../../../core/domain/chat/media';
import {
    IMAGE_BOX_H,
    IMAGE_MAX_W,
    IMAGE_PLACEHOLDER_W,
    STICKER_BOX,
    imageUrlOf,
    str,
} from './model';

const imageSizes = new LruCache<{ width: number; height: number }>(CACHE_MAX);
const resolvedImages = new LruCache<{ url: string; expires: number }>(CACHE_MAX);

export function imageResourceKey(data: Record<string, unknown>): string {
    // 同名附件不是同一张图；回执替换草稿时也不能沿用上一资源的 img 状态。
    return JSON.stringify([
        str(data.file_id),
        str(data.local_file),
        str(data.inline_ref),
        imageCacheKey(str(data.base64)),
        imageCacheKey(str(data.file)),
        imageCacheKey(imageUrlOf(data)),
    ]);
}

function imageSizeFileKey(data: Record<string, unknown>): string {
    const id = str(data.file_id);
    if (id && id !== '0') return id;
    const file = str(data.file);
    // 收藏表情经常全部叫 0；只有内容摘要能跨签名 URL 复用尺寸。
    return /^[a-f0-9]{32}(?:\.[a-z0-9]+)?$/i.test(file.replace(/[{}-]/g, '')) ? file : '';
}

function fitImage(w: number, h: number, sticker = false) {
    const scale = Math.min(
        1,
        (sticker ? STICKER_BOX : IMAGE_MAX_W) / w,
        (sticker ? STICKER_BOX : IMAGE_BOX_H) / h,
    );
    return {
        width: Math.max(1, Math.round(w * scale)),
        height: Math.max(1, Math.round(h * scale)),
    };
}

function sizeOf(data: Record<string, unknown>, url: string, sticker = false, scope = '') {
    const prefix = `${scope}/${sticker ? 'sticker:' : ''}`;
    const key = `${prefix}${imageCacheKey(url)}`;
    const file = imageSizeFileKey(data);
    const cached =
        imageSizes.get(`${prefix}resource:${imageResourceKey(data)}`) ||
        (file && imageSizes.get(`${prefix}file:${imageCacheKey(file)}`)) ||
        imageSizes.get(key);
    if (cached) return cached;
    const width = Number(data.width ?? data.image_width ?? data.pic_width);
    const height = Number(data.height ?? data.image_height ?? data.pic_height);
    return Number.isFinite(width) && width > 0 && Number.isFinite(height) && height > 0
        ? fitImage(width, height, sticker)
        : undefined;
}

async function readImageWithTimeout(
    read: (
        data: Record<string, unknown>,
        refresh?: boolean,
        options?: ImageReadOptions,
    ) => Promise<string>,
    data: Record<string, unknown>,
    refresh?: boolean,
    signal?: AbortSignal,
    queued = false,
    context?: ImageSourceContext,
) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const deadline = new Promise<string>((_, reject) => {
        if (!queued) timer = setTimeout(() => reject(new Error('图片读取超时')), 15_000);
    });
    try {
        const value = await Promise.race([read(data, refresh, { signal, context }), deadline]);
        if (!/^(https?:\/\/|data:image\/|blob:)/i.test(value)) throw new Error('图片地址不可用');
        return value;
    } finally {
        clearTimeout(timer);
    }
}

export function ImageSeg({
    data,
    summary,
    sticker,
    context,
}: {
    data: Record<string, unknown>;
    summary: string;
    sticker?: boolean;
    context: ImageSourceContext;
}) {
    const {
        openImage,
        readImage,
        isImageSourceAlive,
        mediaScope = '',
        imageReadsQueued = false,
    } = useChatView();
    const reader = useRef(readImage);
    reader.current = readImage;
    const request = useRef(0);
    const activeRead = useRef<AbortController | null>(null);
    const canRead = !!readImage;
    const resourceKey = `${mediaScope}/${imageResourceKey(data)}`;
    const cachedUrl = resolvedImages.get(resourceKey);
    // 自己刚发的图带 local_file：先读本机字节，不赌回显的远端地址是否还有效。
    const initialUrl = str(data.local_file)
        ? ''
        : cachedUrl && cachedUrl.expires > Date.now()
          ? cachedUrl.url
          : imageUrlOf(data);
    const viewable = /^(https?:|data:image\/|blob:)/i.test(initialUrl);
    const [url, setUrl] = useState(initialUrl);
    const [refreshing, setRefreshing] = useState(false);
    const refreshAttempts = useRef(0);
    const [attempt, setAttempt] = useState(0);
    const cacheKey = useMemo(() => imageCacheKey(url), [url]);
    const [failed, setFailed] = useState(() => !viewable && !readImage);
    const [size, setSize] = useState(() => sizeOf(data, initialUrl, sticker, mediaScope));
    const [loaded, setLoaded] = useState(false);

    useEffect(() => {
        request.current++;
        setUrl(initialUrl);
        refreshAttempts.current = 0;
        setRefreshing(false);
        setLoaded(false);
        setFailed(!/^(https?:|data:image\/|blob:)/i.test(initialUrl) && !canRead);
        setSize(sizeOf(data, initialUrl, sticker, mediaScope));
        return () => {
            request.current++;
            activeRead.current?.abort();
        };
        // 档案合并会创建等价 data；只有资源身份变化才重新开始加载。
    }, [resourceKey, canRead, sticker]);

    useEffect(() => {
        if (url || !reader.current || failed || refreshing) return;
        const current = ++request.current;
        const controller = new AbortController();
        activeRead.current = controller;
        setRefreshing(true);
        void readImageWithTimeout(
            reader.current,
            data,
            undefined,
            controller.signal,
            imageReadsQueued,
            context,
        )
            .then((value) => {
                if (current === request.current) {
                    setUrl(value);
                    setFailed(false);
                }
            })
            .catch(() => {
                if (current === request.current) setFailed(true);
            })
            .finally(() => {
                if (activeRead.current === controller) activeRead.current = null;
                if (current === request.current) setRefreshing(false);
            });
    }, [resourceKey, failed, canRead, refreshing, url]);

    const refresh = (manual = false) => {
        if (refreshing) return;
        resolvedImages.delete(resourceKey);
        if (manual) refreshAttempts.current = 0;
        if (!readImage) {
            if (manual && url) {
                setFailed(false);
                setLoaded(false);
                setAttempt((value) => value + 1);
            } else setFailed(true);
            return;
        }
        if (refreshAttempts.current >= 2) {
            setFailed(true);
            return;
        }
        refreshAttempts.current++;
        setRefreshing(true);
        const current = ++request.current;
        const controller = new AbortController();
        activeRead.current = controller;
        void (async () => {
            try {
                let value: string;
                try {
                    value = await readImageWithTimeout(
                        readImage,
                        data,
                        true,
                        controller.signal,
                        imageReadsQueued,
                        context,
                    );
                } catch (error) {
                    if (
                        current !== request.current ||
                        controller.signal.aborted ||
                        refreshAttempts.current >= 2
                    )
                        throw error;
                    refreshAttempts.current++;
                    value = await readImageWithTimeout(
                        readImage,
                        data,
                        true,
                        controller.signal,
                        imageReadsQueued,
                        context,
                    );
                }
                if (current === request.current) {
                    setUrl(value);
                    setLoaded(false);
                    setAttempt((value) => value + 1);
                    setFailed(false);
                }
            } catch {
                if (current === request.current) setFailed(true);
            } finally {
                if (activeRead.current === controller) activeRead.current = null;
                if (current === request.current) setRefreshing(false);
            }
        })();
    };
    const timeoutRefresh = useRef(refresh);
    timeoutRefresh.current = refresh;
    useEffect(() => {
        if (!url || loaded || refreshing || failed) return;
        const timer = setTimeout(() => timeoutRefresh.current(), 15_000);
        return () => clearTimeout(timer);
    }, [url, loaded, refreshing, failed, attempt]);

    const boxStyle = {
        width: size?.width ?? (sticker ? STICKER_BOX : IMAGE_PLACEHOLDER_W),
        maxWidth: '100%',
        aspectRatio: `${size?.width ?? (sticker ? STICKER_BOX : IMAGE_PLACEHOLDER_W)} / ${size?.height ?? (sticker ? STICKER_BOX : IMAGE_BOX_H)}`,
        overflow: 'hidden',
    };
    const waiting = refreshing || (!url && canRead && !failed);
    if (failed || waiting)
        return waiting ? (
            <span
                className="my-0.5 flex items-center justify-center rounded-md bg-inset text-text-tertiary"
                style={boxStyle}
            >
                <ImageIcon size={18} className="animate-pulse" aria-label="正在读取图片" />
            </span>
        ) : (
            <BrokenImage url={url} style={boxStyle} onRetry={() => refresh(true)} />
        );

    return (
        <button
            type="button"
            onClick={async (e) => {
                e.stopPropagation();
                if (isImageSourceAlive && !isImageSourceAlive(data) && readImage) {
                    if (refreshing) return;
                    const current = ++request.current;
                    const controller = new AbortController();
                    activeRead.current = controller;
                    setRefreshing(true);
                    try {
                        const fresh = await readImageWithTimeout(
                            readImage,
                            data,
                            false,
                            controller.signal,
                            imageReadsQueued,
                            context,
                        );
                        if (current !== request.current) return;
                        setUrl(fresh);
                        setLoaded(false);
                        openImage(fresh);
                    } catch {
                        if (current === request.current) setFailed(true);
                    } finally {
                        if (activeRead.current === controller) activeRead.current = null;
                        if (current === request.current) setRefreshing(false);
                    }
                    return;
                }
                openImage(url);
            }}
            aria-label={sticker ? `看大图：${summary}` : '看大图'}
            className={cn(
                'relative my-0.5 block cursor-zoom-in overflow-hidden rounded-md',
                !loaded && 'bg-inset',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
            )}
            style={boxStyle}
        >
            {!loaded && (
                <span className="absolute inset-0 flex items-center justify-center text-text-disabled">
                    <ImageIcon size={18} aria-hidden />
                </span>
            )}
            <img
                key={`${url}/${attempt}`}
                src={url}
                alt={summary || '图片'}
                loading="eager"
                decoding="async"
                referrerPolicy="no-referrer"
                draggable={false}
                onLoad={(e) => {
                    const img = e.currentTarget;
                    if (!img.naturalWidth || !img.naturalHeight) return;
                    const fitted = fitImage(img.naturalWidth, img.naturalHeight, sticker);
                    imageSizes.set(`${mediaScope}/${sticker ? 'sticker:' : ''}${cacheKey}`, fitted);
                    // 成功地址会过期，解码尺寸仍属于原资源；重挂时不退回猜测的占位。
                    imageSizes.set(
                        `${mediaScope}/${sticker ? 'sticker:' : ''}resource:${imageResourceKey(data)}`,
                        fitted,
                    );
                    const file = imageSizeFileKey(data);
                    if (file)
                        imageSizes.set(
                            `${mediaScope}/${sticker ? 'sticker:' : ''}file:${imageCacheKey(file)}`,
                            fitted,
                        );
                    if (/^https?:\/\//i.test(url) && url.length < 8192)
                        resolvedImages.set(resourceKey, { url, expires: Date.now() + 60_000 });
                    setSize((previous) =>
                        previous?.width === fitted.width && previous.height === fitted.height
                            ? previous
                            : fitted,
                    );
                    setLoaded(true);
                }}
                onError={() => refresh()}
                className={cn(
                    'absolute inset-0 h-full w-full',
                    'object-contain',
                    // 图到了才显出来，淡入只动透明度
                    loaded ? 'opacity-100' : 'opacity-0',
                    'transition-opacity duration-200 motion-reduce:transition-none',
                )}
            />
        </button>
    );
}

function BrokenImage({
    url,
    style,
    onRetry,
}: {
    url: string;
    style: CSSProperties;
    onRetry: () => void;
}) {
    const { copied, copy } = useCopy();
    return (
        <span
            className="my-0.5 flex flex-col items-center justify-center gap-1 rounded-md border border-dashed border-border bg-inset/60 px-2 text-center text-2xs text-text-tertiary"
            style={style}
        >
            <ImageOff size={16} aria-hidden />
            <span>[图片加载失败]</span>
            <button
                type="button"
                onClick={(event) => {
                    event.stopPropagation();
                    onRetry();
                }}
                className="rounded-xs px-1.5 py-0.5 text-text-secondary hover:bg-inset hover:text-text"
            >
                重试图片
            </button>
            {url && (
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        copy(url);
                    }}
                    className="inline-flex items-center gap-1 rounded-xs px-1.5 py-0.5 text-text-secondary hover:bg-inset hover:text-text"
                >
                    {copied ? <Check size={11} aria-hidden /> : <Copy size={11} aria-hidden />}
                    {copied ? '已复制' : '复制地址'}
                </button>
            )}
        </span>
    );
}
