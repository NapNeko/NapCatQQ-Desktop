// 消息段渲染：文字（保留换行、网址可点）、@、表情、图片 / 表情包、回复引用、语音、视频、文件、合并转发、
// json / xml 卡片、markdown、戳一戳；不认识的段显示成 [类型] 小标签，悬停看原始 JSON。
//
// 每个段单独包一层错误边界：上游给了奇怪的形状，只坏这一个段。
// 图片的框高度是固定的（宽度等图到了再按比例定），行高不会因为图加载完而跳，虚拟列表不用返工。

import { memo, useMemo, useState, type ReactNode } from 'react';
import {
    Check,
    Copy,
    ExternalLink,
    FileText,
    Film,
    Forward,
    Hand,
    Image as ImageIcon,
    ImageOff,
    LayoutTemplate,
    Mic,
    Smile,
} from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { SimpleMarkdown, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { messagePreview, segmentPreview, type Segment } from '../../../core/domain/debug/segments';
import { useChatView } from './chatContext';
import { fileSizeLabel, safeJson } from '../../../core/domain/debug/chatFormat';
import { SafeBoundary, useCopy } from './rightParts';
import { CACHE_MAX, ExpiringSet, FAILURE_TTL_MS, LruCache, imageCacheKey } from './boundedCache';

const str = (v: unknown): string => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '');
const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** 只有图（或表情包）的消息不画气泡底，和 QQ 一样 */
export function isPictureOnly(segments: readonly Segment[]): boolean {
    return segments.length > 0 && segments.every((s) => s.type === 'image' || s.type === 'mface');
}

/** 一段文字最多画这么多字；再长的去详情里看，免得一条消息卡住整栏 */
const TEXT_RENDER_CAP = 20_000;

export const SegmentList = memo(function SegmentList({ segments, mine }: { segments: Segment[]; mine: boolean }) {
    if (segments.length === 0) return <span className="text-text-tertiary">（空消息）</span>;
    // 回复段不管排在哪都画在最上面
    const reply = segments.find((s) => s.type === 'reply');
    const rest = reply ? segments.filter((s) => s !== reply) : segments;
    return (
        <>
            {reply && (
                <SafeBoundary>
                    <ReplyQuote seg={reply} mine={mine} />
                </SafeBoundary>
            )}
            {rest.map((seg, i) => (
                <SafeBoundary key={i} fallback={<UnknownChip seg={seg} note="这个段显示不了" />}>
                    <SegmentView seg={seg} mine={mine} />
                </SafeBoundary>
            ))}
        </>
    );
});

function SegmentView({ seg, mine }: { seg: Segment; mine: boolean }) {
    const d = seg.data;
    switch (seg.type) {
        case 'text':
            return <TextSeg text={str(d.text)} />;
        case 'at':
            return <AtSeg qq={str(d.qq)} name={str(d.name)} mine={mine} />;
        case 'face':
            return <FaceSeg id={str(d.id)} />;
        case 'image':
            return <ImageSeg url={imageUrlOf(d)} summary={str(d.summary)} />;
        case 'mface':
            return <ImageSeg url={imageUrlOf(d)} summary={str(d.summary) || '[表情包]'} sticker />;
        case 'record':
            return <MediaChip icon={<Mic size={11} aria-hidden />} label="语音" url={mediaUrlOf(d)} />;
        case 'video':
            return <MediaChip icon={<Film size={11} aria-hidden />} label="视频" url={mediaUrlOf(d)} />;
        case 'file':
            return <FileCard data={d} />;
        case 'forward':
            return <ForwardCard data={d} />;
        case 'json':
        case 'xml':
            return <RichCard seg={seg} />;
        case 'markdown':
            return <MarkdownSeg content={str(d.content) || str(d.data)} />;
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
                <span className="text-text-tertiary">…（还有 {text.length - TEXT_RENDER_CAP} 字，在详情里看全文）</span>
            )}
        </span>
    );
}

function AtSeg({ qq, name, mine }: { qq: string; name: string; mine: boolean }) {
    const { nameOf } = useChatView();
    if (qq === 'all') return <span className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>@全体成员 </span>;
    const shown = name || nameOf(Number(qq)) || qq;
    return (
        <span title={qq} className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>
            @{shown}{' '}
        </span>
    );
}

function FaceSeg({ id }: { id: string }) {
    const valid = /^\d{1,6}$/.test(id);
    const [failedId, setFailedId] = useState<string | null>(null);
    const label = valid ? `QQ 表情 ${id}` : 'QQ 表情';
    if (!valid || failedId === id) return <Chip icon={<Smile size={11} aria-hidden />} title={label}>{valid ? `表情 ${id}` : '表情'}</Chip>;
    // QFace 按 QQNT emojiId 发布腾讯表情资源；固定尺寸避免资源加载后挤动虚拟列表。
    return <img
        src={`https://koishi.js.org/QFace/assets/qq_emoji/${id}/png/${id}.png`}
        alt={label}
        title={label}
        width={24}
        height={24}
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        draggable={false}
        className="mx-0.5 inline-block h-6 w-6 align-middle object-contain"
        onError={() => setFailedId(id)}
    />;
}

// ---------------------------------------------------------------------------
// 图片 / 表情包
// ---------------------------------------------------------------------------

/** 能直接给 <img> 用的地址：url 优先，file 是网址或 base64 时也认 */
function imageUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file]) {
        const s = str(v);
        if (/^(https?:|data:image\/)/i.test(s)) return s;
        if (s.startsWith('base64://')) return `data:image/png;base64,${s.slice('base64://'.length)}`;
    }
    return str(d.url) || str(d.file);
}

function mediaUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file, d.path]) {
        const s = str(v);
        if (s) return s;
    }
    return '';
}

const IMAGE_BOX_H = 128;
const IMAGE_MAX_W = 200;
const IMAGE_MIN_W = 56;
const IMAGE_PLACEHOLDER_W = 160;
const STICKER_BOX = 96;

// 图的显示宽度按地址记住：行被虚拟列表卸了再挂，直接按上次的宽度画，不闪占位。
// 有上限（最近 500 张）；失败记录 5 分钟后过期，网络抖一下不至于这次运行里一直显示「加载失败」
const imageWidths = new LruCache<number>(CACHE_MAX);
const failedImages = new ExpiringSet(CACHE_MAX, FAILURE_TTL_MS);

function fitWidth(w: number, h: number): number {
    if (!(w > 0) || !(h > 0)) return IMAGE_PLACEHOLDER_W;
    const scaled = h > IMAGE_BOX_H ? (w * IMAGE_BOX_H) / h : w;
    return Math.round(Math.min(IMAGE_MAX_W, Math.max(IMAGE_MIN_W, scaled)));
}

function ImageSeg({ url, summary, sticker }: { url: string; summary: string; sticker?: boolean }) {
    const { openImage } = useChatView();
    const viewable = /^(https?:|data:image\/)/i.test(url);
    const cacheKey = useMemo(() => imageCacheKey(url), [url]);
    const [failed, setFailed] = useState(() => !viewable || failedImages.has(cacheKey));
    const [width, setWidth] = useState(() => imageWidths.get(cacheKey));
    const loaded = width !== undefined;

    if (failed) return <BrokenImage url={url} sticker={sticker} />;

    const boxW = sticker ? STICKER_BOX : (width ?? IMAGE_PLACEHOLDER_W);
    const boxH = sticker ? STICKER_BOX : IMAGE_BOX_H;
    return (
        <button
            type="button"
            onClick={(e) => {
                e.stopPropagation();
                openImage(url);
            }}
            aria-label={sticker ? `看大图：${summary}` : '看大图'}
            className={cn(
                'relative my-0.5 block cursor-zoom-in overflow-hidden rounded-md',
                !loaded && 'bg-inset',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
            )}
            style={{ width: boxW, height: boxH }}
        >
            {!loaded && (
                <span className="absolute inset-0 flex items-center justify-center text-text-disabled">
                    <ImageIcon size={18} aria-hidden />
                </span>
            )}
            <img
                src={url}
                alt={summary || '图片'}
                loading="lazy"
                decoding="async"
                referrerPolicy="no-referrer"
                draggable={false}
                onLoad={(e) => {
                    const img = e.currentTarget;
                    const w = fitWidth(img.naturalWidth, img.naturalHeight);
                    imageWidths.set(cacheKey, w);
                    setWidth(w);
                }}
                onError={() => {
                    failedImages.add(cacheKey);
                    setFailed(true);
                }}
                className={cn(
                    'relative h-full w-full',
                    sticker ? 'object-contain' : 'object-cover',
                    // 图到了才显出来，淡入只动透明度
                    loaded ? 'opacity-100' : 'opacity-0',
                    'transition-opacity duration-200 motion-reduce:transition-none',
                )}
            />
        </button>
    );
}

function BrokenImage({ url, sticker }: { url: string; sticker?: boolean }) {
    const { copied, copy } = useCopy();
    return (
        <span
            className="my-0.5 flex flex-col items-center justify-center gap-1 rounded-md border border-dashed border-border bg-inset/60 px-2 text-center text-2xs text-text-tertiary"
            style={{ width: sticker ? STICKER_BOX : IMAGE_PLACEHOLDER_W, height: sticker ? STICKER_BOX : IMAGE_BOX_H }}
        >
            <ImageOff size={16} aria-hidden />
            <span>[图片加载失败]</span>
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
    const text = target ? `${target.senderName}：${messagePreview(target.segments) || '（空消息）'}` : `回复 #${str(seg.data.id)}`;
    return (
        <button
            type="button"
            onClick={(e) => {
                e.stopPropagation();
                if (target) revealMessage(id);
            }}
            title={target ? '跳到被回复的消息' : '被回复的消息不在当前缓冲里'}
            className={cn(
                'mb-1 block w-full max-w-full border-l-2 pl-2 text-left text-2xs leading-relaxed text-text-tertiary',
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

function Chip({ icon, title, children }: { icon?: ReactNode; title?: string; children: ReactNode }) {
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

function Card({ icon, title, sub, children }: { icon: ReactNode; title: ReactNode; sub?: ReactNode; children?: ReactNode }) {
    return (
        <span className="my-0.5 flex w-[220px] max-w-full items-start gap-2 rounded-md border border-border-subtle bg-surface/80 px-2.5 py-2">
            <span className="mt-0.5 shrink-0 text-text-tertiary">{icon}</span>
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="line-clamp-2 break-words text-xs font-medium text-text">{title}</span>
                {sub && <span className="truncate text-2xs text-text-tertiary">{sub}</span>}
                {children}
            </span>
        </span>
    );
}

function FileCard({ data }: { data: Record<string, unknown> }) {
    const name = str(data.name) || str(data.file_name) || str(data.file) || '文件';
    const size = fileSizeLabel(data.file_size ?? data.size);
    return <Card icon={<FileText size={16} aria-hidden />} title={name} sub={size || '文件'} />;
}

function ForwardCard({ data }: { data: Record<string, unknown> }) {
    const { copied, copy } = useCopy();
    const id = str(data.id);
    const count = Array.isArray(data.messages) ? data.messages.length : Array.isArray(data.content) ? data.content.length : 0;
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
            <SimpleMarkdown text={content} emptyFallback="（空的 Markdown）" onOpenLink={openLink} className="text-[13px] leading-5 text-text" />
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
            <TooltipContent side="top" className="max-w-[320px] whitespace-pre-wrap break-all font-mono text-[11px]">
                {note ? `${note}\n` : ''}
                {json.length > 1200 ? `${json.slice(0, 1200)}…` : json}
            </TooltipContent>
        </Tooltip>
    );
}
