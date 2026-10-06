// 表达方式页的零件：一行一条（精选星标一键切）、新建 / 编辑对话框、逐条过一遍的快速精选。

import { useEffect, useRef, useState } from 'react';
import { ArrowDown, ArrowLeft, ArrowRight, Pencil, Star, Trash2 } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Select,
    Spinner,
    TextAreaField,
    TextField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { MaiBotExpression, MaiBotLearningChat } from '../../../../core/ipc/types';
import { FormDialog } from '../entityParts';
import { RowCheck } from '../resourceParts';
import { relativeTime } from './maibotPromptParts';

export const CurateStar: React.FC<{
    curated: boolean;
    disabled?: boolean;
    onToggle: () => void;
    className?: string;
}> = ({ curated, disabled, onToggle, className }) => (
    <button
        type="button"
        aria-pressed={curated}
        aria-label={curated ? '取消精选' : '精选'}
        title={curated ? '已精选，点一下取消' : '精选：麦麦回复时会用它'}
        disabled={disabled}
        onClick={onToggle}
        className={cn(
            'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm transition-colors disabled:opacity-40',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
            curated
                ? 'text-warning hover:bg-warning-soft'
                : 'text-text-disabled hover:bg-inset hover:text-text-secondary',
            className,
        )}
    >
        <Star size={15} strokeWidth={2} fill={curated ? 'currentColor' : 'none'} />
    </button>
);

export const ExpressionRow: React.FC<{
    item: MaiBotExpression;
    selected: boolean;
    showChat: boolean;
    busy: boolean;
    onPick: (shift: boolean) => void;
    onToggleCurated: () => void;
    onEdit: () => void;
    onDelete: () => void;
}> = ({ item, selected, showChat, busy, onPick, onToggleCurated, onEdit, onDelete }) => (
    <div
        className={cn(
            'group flex items-center gap-2.5 rounded-md border px-3 py-2.5 transition-colors',
            selected
                ? 'border-brand/40 bg-brand-soft/30'
                : 'border-border-subtle bg-surface hover:border-border',
        )}
    >
        <RowCheck checked={selected} onPick={onPick} />
        <CurateStar curated={item.curated} disabled={busy} onToggle={onToggleCurated} />
        <button
            type="button"
            onClick={onEdit}
            className="min-w-0 flex-1 text-left focus-visible:outline-none"
        >
            <span className="block truncate text-xs text-text-tertiary">
                当 <span className="text-text-secondary">{item.situation}</span>
            </span>
            <span className="mt-0.5 block text-[13.5px] leading-snug text-text">{item.style}</span>
        </button>
        <span className="hidden shrink-0 text-right text-2xs text-text-tertiary sm:block">
            {showChat && <span className="block max-w-[10rem] truncate">{item.chat_name}</span>}
            <span className="block">{relativeTime(item.last_active)}</span>
        </span>
        <div className="flex shrink-0 items-center opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
            <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                aria-label="编辑"
                onClick={onEdit}
            >
                <Pencil size={13} />
            </Button>
            <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-danger hover:text-danger"
                aria-label="删除"
                disabled={busy}
                onClick={onDelete}
            >
                <Trash2 size={13} />
            </Button>
        </div>
    </div>
);

export type ExpressionDraft = { id?: number; situation: string; style: string; chat_id: string };

export const ExpressionDialog: React.FC<{
    draft: ExpressionDraft | null;
    chats: readonly MaiBotLearningChat[];
    busy: boolean;
    onChange: (next: ExpressionDraft) => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ draft, chats, busy, onChange, onCancel, onConfirm }) => {
    if (!draft) return null;
    const editing = draft.id !== undefined;
    const ok = !!draft.situation.trim() && !!draft.style.trim() && (editing || !!draft.chat_id);
    return (
        <FormDialog
            open
            size="md"
            title={editing ? '改表达方式' : '加一条表达方式'}
            description="麦麦碰到这种情境时，会参考这句说法来回复。"
            confirmLabel={editing ? '保存' : '加上'}
            confirmDisabled={!ok}
            busy={busy}
            onCancel={onCancel}
            onConfirm={onConfirm}
        >
            <TextField
                label="情境"
                autoFocus
                placeholder="比如：有人夸你"
                value={draft.situation}
                onValueChange={(situation) => onChange({ ...draft, situation })}
            />
            <TextAreaField
                label="说法"
                minRows={2}
                placeholder="比如：嘿嘿 被发现了"
                value={draft.style}
                onValueChange={(style) => onChange({ ...draft, style })}
            />
            <Select
                label="学在哪个聊天"
                placeholder="挑一个聊天"
                items={chats.map((c) => ({ value: c.chat_id, label: c.chat_name }))}
                value={draft.chat_id || undefined}
                onValueChange={(chat_id) => onChange({ ...draft, chat_id })}
            />
        </FormDialog>
    );
};

/**
 * 逐条过一遍没精选的：键盘 → 精选、← 删掉、↓ 跳过。上游默认只拿精选的回复，
 * 新学到的攒多了一条条点星标太累，这里一口气过
 */
export const ExpressionReview: React.FC<{
    open: boolean;
    queue: readonly MaiBotExpression[];
    loading: boolean;
    onCurate: (item: MaiBotExpression) => Promise<unknown>;
    onDelete: (item: MaiBotExpression) => Promise<unknown>;
    onClose: () => void;
}> = ({ open, queue, loading, onCurate, onDelete, onClose }) => {
    // 看过的（跳过的、处理掉的）先从队列里拿掉：列表重新拉回来之前，连按两下不会落在同一条上。
    // 所以按钮不必等上一条请求回来，连着点、连着按方向键都行
    const [seen, setSeen] = useState<ReadonlySet<number>>(() => new Set());
    const [done, setDone] = useState(0);
    const curateBtn = useRef<HTMLButtonElement>(null);
    const pending = queue.filter((e) => !seen.has(e.id));
    const current = pending[0];

    useEffect(() => {
        if (!open) {
            setSeen(new Set());
            setDone(0);
        }
    }, [open]);

    const markSeen = (id: number) => setSeen((s) => new Set(s).add(id));
    const act = (fn: (e: MaiBotExpression) => Promise<unknown>) => {
        if (!current) return;
        markSeen(current.id);
        void fn(current).then(() => setDone((n) => n + 1));
    };
    const handlers = useRef({ curate: () => {}, remove: () => {}, skip: () => {} });
    handlers.current = {
        curate: () => act(onCurate),
        remove: () => act(onDelete),
        skip: () => current && markSeen(current.id),
    };
    useEffect(() => {
        if (!open) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'ArrowRight') handlers.current.curate();
            else if (e.key === 'ArrowLeft') handlers.current.remove();
            else if (e.key === 'ArrowDown') handlers.current.skip();
            else return;
            e.preventDefault();
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [open]);
    // 头一回打开还在读，按钮没出来，焦点先落在关闭上；读到了再挪过来
    const hasCurrent = !!current;
    useEffect(() => {
        if (open && hasCurrent && document.activeElement !== curateBtn.current)
            curateBtn.current?.focus();
    }, [open, hasCurrent]);

    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent
                size="md"
                // 默认会落在第一个按钮「删掉」上，回车就删了；落在「精选」上，回车等于往右
                onOpenAutoFocus={(e) => {
                    if (!curateBtn.current) return;
                    e.preventDefault();
                    curateBtn.current.focus();
                }}
            >
                <DialogHeader>
                    <DialogTitle>逐条过一遍</DialogTitle>
                    <p className="text-xs text-text-tertiary">
                        {current ? `还剩 ${pending.length} 条没看` : ''}
                        {done > 0 && `${current ? ' · ' : ''}这次处理了 ${done} 条`}
                    </p>
                </DialogHeader>
                {loading && !current ? (
                    <div className="flex h-40 items-center justify-center">
                        <Spinner size="md" tone="brand" label="正在读取" />
                    </div>
                ) : current ? (
                    <div className="flex flex-col gap-4">
                        <div className="rounded-md border border-border-subtle bg-field px-5 py-5">
                            <p className="text-xs text-text-tertiary">
                                当 <span className="text-text-secondary">{current.situation}</span>
                            </p>
                            <p className="mt-2 font-display text-lg leading-snug text-text">
                                {current.style}
                            </p>
                            <p className="mt-3 text-2xs text-text-tertiary">
                                {current.chat_name} · {relativeTime(current.last_active)}
                            </p>
                        </div>
                        <div className="grid grid-cols-3 gap-2">
                            <Button
                                variant="secondary"
                                onClick={() => handlers.current.remove()}
                                className="text-danger"
                            >
                                <ArrowLeft size={14} />
                                删掉
                            </Button>
                            <Button variant="secondary" onClick={() => handlers.current.skip()}>
                                <ArrowDown size={14} />
                                跳过
                            </Button>
                            <Button
                                ref={curateBtn}
                                variant="primary"
                                onClick={() => handlers.current.curate()}
                            >
                                精选
                                <ArrowRight size={14} />
                            </Button>
                        </div>
                        <p className="text-center text-2xs text-text-tertiary">
                            也可以用方向键：← 删掉 · ↓ 跳过 · → 精选
                        </p>
                    </div>
                ) : (
                    <div className="flex h-40 flex-col items-center justify-center gap-2 text-center">
                        <p className="text-sm text-text">都看完了</p>
                        <p className="text-xs text-text-tertiary">
                            新学到的表达方式会出现在「未精选」里。
                        </p>
                        <Button size="sm" variant="secondary" className="mt-2" onClick={onClose}>
                            关掉
                        </Button>
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
};
