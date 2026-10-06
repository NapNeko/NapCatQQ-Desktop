// 表情包页的零件：一张图（缩略图 / 原图，自己去要）、网格里的一格、点开后的详情（大图、标签、收下丢弃，
// 左右键翻上一张下一张）。

import { useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, ImageOff, Trash2 } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogTitle,
    Spinner,
    StringListField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    EMOJI_STATUS_LABEL,
    emojiMoves,
    normalizeEmojiTags,
    type EmojiMove,
} from '../../../../core/domain/apps/maibotEmoji';
import type { MaiBotEmoji, MaiBotEmojiStatus } from '../../../../core/ipc/types';
import { useMaiBotEmojiImage } from '../../../../hooks/apps/useMaiBotEmojis';
import { RowCheck } from '../resourceParts';
import { relativeTime } from './maibotPromptParts';

const STATUS_DOT: Record<MaiBotEmojiStatus, string> = {
    adopted: 'bg-success',
    known: 'bg-info',
    unknown: 'bg-text-disabled',
    discarded: 'bg-danger',
};

export const StatusDot: React.FC<{ status: MaiBotEmojiStatus }> = ({ status }) => (
    <span
        aria-hidden
        className={cn('inline-block h-1.5 w-1.5 shrink-0 rounded-full', STATUS_DOT[status])}
    />
);

/** 图自己去要：没到时是一块呼吸的底，文件被清理过显示「图没了」 */
export const EmojiImage: React.FC<{
    instanceId: string;
    emoji: MaiBotEmoji;
    original?: boolean;
    className?: string;
}> = ({ instanceId, emoji, original = false, className }) => {
    const img = useMaiBotEmojiImage(instanceId, emoji.id, original);
    // 原图还没到时先拿缓存里的缩略图垫着，不闪一下空白
    const thumb = useMaiBotEmojiImage(instanceId, emoji.id, false, original && !img.data);
    const src = img.data?.data_url ?? (original ? thumb.data?.data_url : undefined);
    if (src) {
        return (
            <img
                src={src}
                alt={emoji.tags.join('、')}
                draggable={false}
                className={cn(
                    'h-full w-full object-contain',
                    emoji.status === 'discarded' && 'opacity-50 grayscale',
                    className,
                )}
            />
        );
    }
    if (img.isPending || (img.isFetching && !img.data)) {
        return (
            <span
                className={cn('block h-full w-full animate-pulse rounded-sm bg-inset', className)}
            />
        );
    }
    return (
        <span
            className={cn(
                'flex h-full w-full flex-col items-center justify-center gap-1 text-text-disabled',
                className,
            )}
        >
            <ImageOff size={original ? 28 : 18} />
            <span className="text-2xs">{img.isError ? '没取到图' : '图没了'}</span>
        </span>
    );
};

export const EmojiTile: React.FC<{
    instanceId: string;
    emoji: MaiBotEmoji;
    selected: boolean;
    /** 已经有勾上的：每格都露出勾选框 */
    selecting: boolean;
    onPick: (shift: boolean) => void;
    onOpen: () => void;
}> = ({ instanceId, emoji: e, selected, selecting, onPick, onOpen }) => (
    <div
        className={cn(
            'group relative flex flex-col overflow-hidden rounded-md border bg-surface transition-[border-color,box-shadow]',
            selected
                ? 'border-brand/60 ring-2 ring-brand/25'
                : 'border-border-subtle hover:border-border hover:shadow-sm',
        )}
    >
        <button
            type="button"
            onClick={onOpen}
            className="relative block aspect-square bg-field p-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand/50"
        >
            <EmojiImage instanceId={instanceId} emoji={e} />
            {e.format === 'gif' && (
                <span className="absolute right-1.5 top-1.5 rounded-xs bg-black/55 px-1 py-px text-[10px] font-semibold leading-none text-white">
                    GIF
                </span>
            )}
        </button>
        <div
            className={cn(
                'absolute left-1.5 top-1.5 rounded-xs bg-surface/90 p-0.5 transition-opacity',
                !selecting &&
                    !selected &&
                    'opacity-0 focus-within:opacity-100 group-hover:opacity-100',
            )}
        >
            <RowCheck checked={selected} onPick={onPick} />
        </div>
        <button type="button" onClick={onOpen} tabIndex={-1} className="px-2.5 py-2 text-left">
            <span
                className={cn(
                    'block truncate text-xs',
                    e.tags.length ? 'text-text' : 'text-text-tertiary',
                )}
            >
                {e.tags.length ? e.tags.join(' · ') : '没标签'}
            </span>
            <span className="mt-0.5 flex items-center gap-1 text-2xs text-text-tertiary">
                <StatusDot status={e.status} />
                {EMOJI_STATUS_LABEL[e.status]}
                {e.usage_count > 0 && <span className="truncate"> · 发过 {e.usage_count} 次</span>}
            </span>
        </button>
    </div>
);

const MOVE_LABEL: Record<EmojiMove, string> = {
    adopt: '收下',
    unadopt: '不再发',
    discard: '丢弃',
    restore: '捡回来',
};

/**
 * 点开一张：大图（原图，动图会动）、标签随改随存、按状态给收下 / 丢弃这些。
 * ← → 翻同一页的上一张下一张，焦点在标签输入框里时不翻
 */
export const EmojiDetail: React.FC<{
    instanceId: string;
    emoji: MaiBotEmoji | null;
    position: { index: number; total: number } | null;
    busy: boolean;
    onPrev?: () => void;
    onNext?: () => void;
    onTags: (tags: string[]) => void;
    onMove: (move: EmojiMove) => void;
    onDelete: () => void;
    onClose: () => void;
}> = ({ instanceId, emoji, position, busy, onPrev, onNext, onTags, onMove, onDelete, onClose }) => {
    const [tags, setTags] = useState<string[]>([]);
    const shownId = useRef<number | null>(null);
    // 换了一张才重置输入：同一张刷新回来（刚存过标签）不打断正在打的字
    useEffect(() => {
        if (emoji && emoji.id !== shownId.current) {
            shownId.current = emoji.id;
            setTags(emoji.tags);
        }
    }, [emoji]);

    const nav = useRef({ prev: onPrev, next: onNext });
    nav.current = { prev: onPrev, next: onNext };
    const open = emoji !== null;
    useEffect(() => {
        if (!open) return;
        const onKey = (ev: KeyboardEvent) => {
            const t = ev.target as HTMLElement | null;
            if (t?.closest('input, textarea, [contenteditable=true]')) return;
            if (ev.key === 'ArrowLeft') nav.current.prev?.();
            else if (ev.key === 'ArrowRight') nav.current.next?.();
            else return;
            ev.preventDefault();
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [open]);

    const e = emoji;
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="lg">
                <DialogTitle className="sr-only">表情包</DialogTitle>
                {e && (
                    <div className="flex flex-col gap-4">
                        <div className="relative flex h-72 items-center justify-center rounded-md bg-field p-4">
                            <EmojiImage
                                instanceId={instanceId}
                                emoji={e}
                                original
                                className="max-h-64"
                            />
                            <NavButton side="left" label="上一张" onClick={onPrev} />
                            <NavButton side="right" label="下一张" onClick={onNext} />
                        </div>
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-text-tertiary">
                            <span className="inline-flex items-center gap-1.5 text-sm font-medium text-text">
                                <StatusDot status={e.status} />
                                {EMOJI_STATUS_LABEL[e.status]}
                            </span>
                            <span className="uppercase">{e.format}</span>
                            {e.usage_count > 0 && <span>发过 {e.usage_count} 次</span>}
                            {e.last_used && <span>上次 {relativeTime(e.last_used)}</span>}
                            {e.found_at && <span>{relativeTime(e.found_at)}记下</span>}
                            {position && (
                                <span className="ml-auto tabular-nums">
                                    {position.index + 1} / {position.total}
                                </span>
                            )}
                        </div>
                        <StringListField
                            label={
                                <span className="inline-flex items-center gap-2">
                                    情绪标签
                                    {busy && <Spinner size="sm" />}
                                </span>
                            }
                            hint="麦麦按要回的情绪和各张的标签比，挑最像的发；没标签的挑不中"
                            mono={false}
                            placeholder="比如：开心、得意，回车加上"
                            value={tags}
                            onChange={(next) => {
                                const clean = normalizeEmojiTags(next);
                                setTags(clean);
                                onTags(clean);
                            }}
                        />
                        <div className="flex flex-wrap items-center gap-2">
                            {emojiMoves(e.status).map((m) => (
                                <Button
                                    key={m}
                                    size="sm"
                                    variant={
                                        m === 'adopt' || m === 'restore' ? 'primary' : 'secondary'
                                    }
                                    disabled={busy}
                                    onClick={() => onMove(m)}
                                >
                                    {MOVE_LABEL[m]}
                                </Button>
                            ))}
                            <span className="flex-1" />
                            <Button
                                size="sm"
                                variant="ghost"
                                className="text-danger hover:bg-danger-soft hover:text-danger"
                                disabled={busy}
                                onClick={onDelete}
                            >
                                <Trash2 size={13} />
                                删除
                            </Button>
                        </div>
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
};

const NavButton: React.FC<{ side: 'left' | 'right'; label: string; onClick?: () => void }> = ({
    side,
    label,
    onClick,
}) =>
    onClick ? (
        <button
            type="button"
            aria-label={label}
            onClick={onClick}
            className={cn(
                'absolute top-1/2 inline-flex h-9 w-9 -translate-y-1/2 items-center justify-center rounded-full bg-surface/85 text-text-secondary shadow-sm ring-1 ring-border-subtle transition-colors hover:bg-surface hover:text-text',
                side === 'left' ? 'left-3' : 'right-3',
            )}
        >
            {side === 'left' ? <ChevronLeft size={18} /> : <ChevronRight size={18} />}
        </button>
    ) : null;
