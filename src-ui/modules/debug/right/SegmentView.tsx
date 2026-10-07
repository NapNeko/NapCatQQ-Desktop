// 消息段渲染：文字（保留换行、网址可点）、@、表情、图片 / 表情包、回复引用、语音、视频、文件、合并转发、
// json / xml 卡片、markdown、戳一戳；不认识的段显示成 [类型] 小标签，悬停看原始 JSON。
//
// 每个段单独包一层错误边界：上游给了奇怪的形状，只坏这一个段。
// 图片完整等比缩放并缓存尺寸，虚拟列表重新挂载时复用已知宽高。

import {
    memo,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type CSSProperties,
} from 'react';
import {
    Check,
    Copy,
    ExternalLink,
    FileText,
    Forward,
    Hand,
    Image as ImageIcon,
    ImageOff,
    LayoutTemplate,
    Mic,
} from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { SimpleMarkdown, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { messagePreview, segmentPreview, type Segment } from '../../../core/domain/debug/segments';
import { markdownContent } from '../../../core/domain/debug/markdown';
import { useChatView } from './chatContext';
import { fileSizeLabel, safeJson } from '../../../core/domain/debug/chatFormat';
import { SafeBoundary, useCopy } from './rightParts';
import { CACHE_MAX, LruCache, imageCacheKey } from './boundedCache';
import { QQFace } from '../../chat/media/QQFace';
import { qqFaceLarge, type QQFaceDisplaySegment } from '../../../core/domain/chat/qqFaces';
import { projectMessageDisplay } from '../../../core/domain/chat/messageDisplay';
import { qqFaceService } from '../../../core/services/qq-face.service';
import type {
    ImageReadOptions,
    ImageSourceContext,
} from '../../../core/services/chat-media.service';
import { ChatForward } from '../../chat/media/ChatForward';
import { ChatRecord } from '../../chat/media/ChatRecord';
import { ChatVideo } from '../../chat/media/ChatVideo';
import { ChatAvatar } from '../../chat/ChatAvatar';

const str = (v: unknown): string =>
    typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '';
const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/** 只有图（或表情包）的消息不画气泡底，和 QQ 一样 */
export function isPictureOnly(segments: readonly Segment[]): boolean {
    return (
        segments.length > 0 &&
        projectMessageDisplay(segments).every((s) => s.type === 'image' || s.type === 'mface')
    );
}

/** 图片、表情、语音、视频和合并转发单独成消息时，让内容直接贴在时间线上。 */
export function isMediaOnly(segments: readonly Segment[]): boolean {
    return (
        segments.length > 0 &&
        projectMessageDisplay(segments, qqFaceService.peek).every(
            (s) =>
                ['image', 'mface', 'record', 'video', 'forward'].includes(s.type) ||
                (s.type === 'face' && (qqFaceLarge(s.data) ?? s.displayLarge)),
        )
    );
}

/** 一段文字最多画这么多字；再长的去详情里看，免得一条消息卡住整栏 */
const TEXT_RENDER_CAP = 20_000;

export const SegmentList = memo(function SegmentList({
    segments,
    mine,
    messageId,
    imageIndexOffset = 0,
}: {
    segments: Segment[];
    mine: boolean;
    messageId?: string;
    imageIndexOffset?: number;
}) {
    if (segments.length === 0) return <span className="text-text-tertiary">（空消息）</span>;
    // 回复段不管排在哪都画在最上面
    const reply = segments.find((s) => s.type === 'reply');
    const displayed = projectMessageDisplay(segments, qqFaceService.peek);
    const rest = reply ? displayed.filter((s) => s !== reply) : displayed;
    let imageIndex = imageIndexOffset;
    return (
        <>
            {reply && (
                <SafeBoundary>
                    <ReplyQuote seg={reply} mine={mine} />
                </SafeBoundary>
            )}
            {rest.map((seg, i) => {
                const ordinal = imageIndex;
                if (seg.type === 'image' || seg.type === 'mface') imageIndex++;
                return (
                    <SafeBoundary
                        key={i}
                        fallback={<UnknownChip seg={seg} note="这个段显示不了" />}
                    >
                        <SegmentView
                            seg={seg}
                            mine={mine}
                            messageId={messageId}
                            imageIndex={ordinal}
                        />
                    </SafeBoundary>
                );
            })}
        </>
    );
});

function SegmentView({
    seg,
    mine,
    messageId,
    imageIndex,
}: {
    seg: QQFaceDisplaySegment;
    mine: boolean;
    messageId?: string;
    imageIndex: number;
}) {
    const d = seg.data;
    switch (seg.type) {
        case 'text':
            return <TextSeg text={str(d.text)} />;
        case 'at':
            return <AtSeg qq={str(d.qq)} name={str(d.name)} mine={mine} />;
        case 'face':
            return <QQFace id={str(d.id)} data={d} displayLarge={seg.displayLarge} animated />;
        case 'image':
            return (
                <ImageSeg
                    key={imageResourceKey(d)}
                    data={d}
                    context={{ messageId, imageIndex }}
                    summary={str(d.summary)}
                    sticker={Number(d.sub_type ?? d.subType) === 1}
                />
            );
        case 'mface':
            return (
                <ImageSeg
                    key={imageResourceKey(d)}
                    data={d}
                    context={{ messageId, imageIndex }}
                    summary={str(d.summary) || '[表情包]'}
                    sticker
                />
            );
        case 'record':
            return (
                <ChatRecord
                    data={d}
                    messageId={messageId}
                    fallback={
                        <MediaChip
                            icon={<Mic size={11} aria-hidden />}
                            label="语音"
                            url={mediaUrlOf(d)}
                        />
                    }
                />
            );
        case 'video':
            return <ChatVideo data={d} />;
        case 'file':
            return <FileCard data={d} />;
        case 'forward':
            return <ForwardCard data={d} />;
        case 'json':
        case 'xml':
            return <RichCard seg={seg} />;
        case 'markdown':
            return <MarkdownSeg content={markdownContent(d)} />;
        case 'poke':
            return <Chip icon={<Hand size={11} aria-hidden />}>戳了戳</Chip>;
        default:
            return <UnknownChip seg={seg} />;
    }
}

// ---------------------------------------------------------------------------
// 文字
// ---------------------------------------------------------------------------

// 网址后面紧跟的中文标点、右括号不算网址的一部分
const URL_RE = /https?:\/\/[^\s<>"'`，。！？、；：（）【】《》「」)\]}]+/g;

function TextSeg({ text }: { text: string }) {
    const { openLink } = useChatView();
    const shown = text.length > TEXT_RENDER_CAP ? text.slice(0, TEXT_RENDER_CAP) : text;
    const parts = useMemo(() => {
        const out: Array<{ link: boolean; text: string }> = [];
        let last = 0;
        URL_RE.lastIndex = 0;
        for (let m = URL_RE.exec(shown); m; m = URL_RE.exec(shown)) {
            if (m.index > last) out.push({ link: false, text: shown.slice(last, m.index) });
            out.push({ link: true, text: m[0] });
            last = m.index + m[0].length;
        }
        if (last < shown.length) out.push({ link: false, text: shown.slice(last) });
        return out;
    }, [shown]);
    return (
        <span className="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
            {parts.map((p, i) =>
                p.link ? (
                    <a
                        key={i}
                        href={p.text}
                        onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            openLink(p.text);
                        }}
                        className="text-info underline-offset-2 hover:underline"
                    >
                        {p.text}
                    </a>
                ) : (
                    <span key={i}>{p.text}</span>
                ),
            )}
            {shown !== text && (
                <span className="text-text-tertiary">
                    …（还有 {text.length - TEXT_RENDER_CAP} 字，在详情里看全文）
                </span>
            )}
        </span>
    );
}

function AtSeg({ qq, name, mine }: { qq: string; name: string; mine: boolean }) {
    const { nameOf } = useChatView();
    if (qq === 'all')
        return (
            <span className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>@全体成员 </span>
        );
    const shown = name || nameOf(Number(qq)) || qq;
    return (
        <span
            title={qq}
            className={cn(
                'inline-flex items-center gap-1 align-middle font-medium',
                mine ? 'text-brand' : 'text-info',
            )}
        >
            <ChatAvatar contact={{ type: 'private', id: qq, name: shown }} inline />
            <span>@{shown} </span>
        </span>
    );
}

// ---------------------------------------------------------------------------
// 图片 / 表情包
// ---------------------------------------------------------------------------

/** 能直接给 <img> 用的地址：url 优先，file 是网址或 base64 时也认 */
function imageUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file]) {
        const s = str(v);
        if (/^(https?:|data:image\/)/i.test(s)) return s;
        if (s.startsWith('base64://'))
            return `data:image/png;base64,${s.slice('base64://'.length)}`;
    }
    return '';
}

function mediaUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file, d.path]) {
        const s = str(v);
        if (s) return s;
    }
    return '';
}

const IMAGE_BOX_H = 280;
const IMAGE_MAX_W = 320;
const IMAGE_PLACEHOLDER_W = IMAGE_MAX_W;
const STICKER_BOX = 128;

// 图的显示尺寸按地址记住：行被虚拟列表卸了再挂，直接按上次的尺寸画，不闪占位。
// 缓存有上限；短期复用刷新后的地址，不缓存网络失败。
const imageSizes = new LruCache<{ width: number; height: number }>(CACHE_MAX);
const resolvedImages = new LruCache<{ url: string; expires: number }>(CACHE_MAX);

function imageResourceKey(data: Record<string, unknown>): string {
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

function ImageSeg({
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

// ---------------------------------------------------------------------------
// 回复引用
// ---------------------------------------------------------------------------

function ReplyQuote({ seg, mine }: { seg: Segment; mine: boolean }) {
    const { findMessage, revealMessage } = useChatView();
    const id = Number(str(seg.data.id));
    const target = Number.isFinite(id) ? findMessage(id) : undefined;
    const text = target
        ? `${target.senderName}：${messagePreview(target.segments) || '（空消息）'}`
        : `回复 #${str(seg.data.id)}`;
    return (
        <button
            type="button"
            onClick={(e) => {
                e.stopPropagation();
                if (target) revealMessage(id);
            }}
            title={target ? '跳到被回复的消息' : '被回复的消息不在当前缓冲里'}
            className={cn(
                'mb-1 block w-fit max-w-full border-l-2 pl-2 text-left text-2xs leading-relaxed text-text-tertiary',
                mine ? 'border-brand/40' : 'border-border',
                target ? 'cursor-pointer hover:text-text-secondary' : 'cursor-default',
            )}
        >
            <span className="line-clamp-2 break-words">{text}</span>
        </button>
    );
}

// ---------------------------------------------------------------------------
// 小标签和卡片
// ---------------------------------------------------------------------------

function Chip({
    icon,
    title,
    children,
}: {
    icon?: ReactNode;
    title?: string;
    children: ReactNode;
}) {
    return (
        <span
            title={title}
            className="mx-0.5 inline-flex items-center gap-1 rounded-xs bg-inset px-1.5 py-px align-[1px] text-2xs text-text-secondary"
        >
            {icon}
            {children}
        </span>
    );
}

function MediaChip({ icon, label, url }: { icon: ReactNode; label: string; url: string }) {
    const { copied, copy } = useCopy();
    return (
        <span className="mx-0.5 inline-flex items-center gap-1 rounded-xs bg-inset px-1.5 py-px align-[1px] text-2xs text-text-secondary">
            {icon}
            {label}
            {url && (
                <button
                    type="button"
                    aria-label={`复制${label}地址`}
                    title={url}
                    onClick={(e) => {
                        e.stopPropagation();
                        copy(url);
                    }}
                    className="ml-0.5 inline-flex h-4 w-4 items-center justify-center rounded-xs text-text-tertiary hover:bg-surface hover:text-text"
                >
                    {copied ? <Check size={10} aria-hidden /> : <Copy size={10} aria-hidden />}
                </button>
            )}
        </span>
    );
}

function Card({
    icon,
    title,
    sub,
    children,
}: {
    icon: ReactNode;
    title: ReactNode;
    sub?: ReactNode;
    children?: ReactNode;
}) {
    return (
        <span className="my-0.5 flex w-[220px] max-w-full items-start gap-2 rounded-md border border-border-subtle bg-surface/80 px-2.5 py-2">
            <span className="mt-0.5 shrink-0 text-text-tertiary">{icon}</span>
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="line-clamp-2 break-words text-xs font-medium text-text">
                    {title}
                </span>
                {sub && <span className="truncate text-2xs text-text-tertiary">{sub}</span>}
                {children}
            </span>
        </span>
    );
}

function FileCard({ data }: { data: Record<string, unknown> }) {
    const name = str(data.name) || str(data.file_name) || str(data.file) || '文件';
    const size = fileSizeLabel(data.file_size ?? data.size);
    const { fileAction } = useChatView();
    return (
        <Card icon={<FileText size={16} aria-hidden />} title={name} sub={size || '文件'}>
            {fileAction?.(data)}
        </Card>
    );
}

function ForwardCard({ data }: { data: Record<string, unknown> }) {
    const { readForward } = useChatView();
    const { copied, copy } = useCopy();
    const id = str(data.id);
    const count = Array.isArray(data.messages)
        ? data.messages.length
        : Array.isArray(data.content)
          ? data.content.length
          : 0;
    if (readForward)
        return (
            <ChatForward
                data={data}
                read={readForward}
                renderSegments={(segments) => <SegmentList mine={false} segments={segments} />}
            />
        );
    return (
        <Card
            icon={<Forward size={16} aria-hidden />}
            title="聊天记录"
            sub={count > 0 ? `${count} 条消息` : id ? `id ${id}` : undefined}
        >
            {id && (
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        copy(id);
                    }}
                    className="mt-0.5 inline-flex w-fit items-center gap-1 text-2xs text-text-tertiary hover:text-text"
                >
                    {copied ? <Check size={10} aria-hidden /> : <Copy size={10} aria-hidden />}
                    {copied ? '已复制' : '复制 id（get_forward_msg 用）'}
                </button>
            )}
        </Card>
    );
}

/** json 卡片里常见的跳转地址 */
function cardLink(obj: Record<string, unknown>): string {
    const meta = isRecord(obj.meta) ? obj.meta : {};
    for (const v of Object.values(meta)) {
        if (!isRecord(v)) continue;
        for (const k of ['jumpUrl', 'qqdocurl', 'url']) {
            const s = str(v[k]);
            if (/^https?:\/\//i.test(s)) return s;
        }
    }
    return '';
}

function RichCard({ seg }: { seg: Segment }) {
    const { openLink } = useChatView();
    const info = useMemo(() => {
        // 标题沿用预览的口径（prompt → meta.*.title / desc → brief），去掉前缀
        const title = segmentPreview(seg).replace(/^\[卡片\]\s*/, '');
        let app = '';
        let link = '';
        if (seg.type === 'json' && typeof seg.data.data === 'string') {
            try {
                const obj: unknown = JSON.parse(seg.data.data);
                if (isRecord(obj)) {
                    app = str(obj.app);
                    link = cardLink(obj);
                }
            } catch {
                // 解析不了就只显示标题
            }
        }
        return { title: title === '[卡片]' ? '' : title, app, link };
    }, [seg]);
    return (
        <Card
            icon={<LayoutTemplate size={16} aria-hidden />}
            title={info.title || (seg.type === 'json' ? 'JSON 卡片' : 'XML 卡片')}
            sub={info.app || (seg.type === 'json' ? 'json' : 'xml')}
        >
            {info.link && (
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        openLink(info.link);
                    }}
                    className="mt-0.5 inline-flex w-fit items-center gap-1 text-2xs text-info hover:underline"
                >
                    <ExternalLink size={10} aria-hidden />
                    打开链接
                </button>
            )}
        </Card>
    );
}

function MarkdownSeg({ content }: { content: string }) {
    const { openLink } = useChatView();
    return (
        // 不在这里另外截高：气泡的 12 行折叠 +「展开」管它
        <span className="my-0.5 block">
            <SimpleMarkdown
                text={content}
                emptyFallback="（空的 Markdown）"
                onOpenLink={openLink}
                className="text-[13px] leading-5 text-text"
            />
        </span>
    );
}

function UnknownChip({ seg, note }: { seg: Segment; note?: string }) {
    const json = safeJson(seg.data);
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <span
                    tabIndex={0}
                    className="mx-0.5 inline-flex cursor-help items-center rounded-xs border border-dashed border-border px-1.5 py-px align-[1px] font-mono text-[11px] text-text-tertiary"
                >
                    [{seg.type || '?'}]
                </span>
            </TooltipTrigger>
            <TooltipContent
                side="top"
                className="max-w-[320px] whitespace-pre-wrap break-all font-mono text-[11px]"
            >
                {note ? `${note}\n` : ''}
                {json.length > 1200 ? `${json.slice(0, 1200)}…` : json}
            </TooltipContent>
        </Tooltip>
    );
}
