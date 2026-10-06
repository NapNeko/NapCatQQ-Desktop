// 知识库三页（导入 / 浏览 / 图谱）共用的零件：切页、记忆类型的名字和图标、一行一条记忆、
// 记忆详情、先预览再删的确认框。

import { useState, type ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { BookOpenText, Boxes, GitFork, Lightbulb, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type {
    MaiBotMemoryDeleteResult,
    MaiBotMemoryDeleteTarget,
    MaiBotMemoryRecord,
    MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind,
} from '../../../../core/ipc/types';
import { useMaiBotMemoryDelete } from '../../../../hooks/apps/useMaiBotMemory';
import { RowCheck, Segmented } from '../resourceParts';
import { relativeTime } from './maibotPromptParts';

export type KnowledgeView = 'import' | 'browse' | 'graph';

export const KnowledgeSwitch: React.FC<{
    view: KnowledgeView;
    onView: (v: KnowledgeView) => void;
}> = ({ view, onView }) => (
    <Segmented
        items={[
            { value: 'import', label: '导入' },
            { value: 'browse', label: '浏览' },
            { value: 'graph', label: '图谱' },
        ]}
        value={view}
        onChange={onView}
    />
);

export const KIND_LABEL: Readonly<Record<MaiBotMemoryRecordKind, string>> = {
    paragraph: '段落',
    entity: '实体',
    relation: '关系',
    fact: '事实',
};

const KIND_ICON: Readonly<Record<MaiBotMemoryRecordKind, ComponentType<LucideProps>>> = {
    paragraph: BookOpenText,
    entity: Boxes,
    relation: GitFork,
    fact: Lightbulb,
};

const KIND_TONE: Readonly<Record<MaiBotMemoryRecordKind, string>> = {
    paragraph: 'bg-info-soft text-info',
    entity: 'bg-brand-soft text-brand',
    relation: 'bg-success-soft text-success',
    fact: 'bg-warning-soft text-warning',
};

export const KindIcon: React.FC<{ kind: MaiBotMemoryRecordKind; className?: string }> = ({
    kind,
    className,
}) => {
    const Icon = KIND_ICON[kind];
    return (
        <span
            className={cn(
                'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm',
                KIND_TONE[kind],
                className,
            )}
        >
            <Icon size={14} />
        </span>
    );
};

/** 行里第二行：上游的 summary 是给开发看的（「置信度 0.86」「person · 一串 id」），换成人话 */
function recordSubtitle(r: MaiBotMemoryRecord): string {
    switch (r.kind) {
        case 'paragraph':
            return r.source || '没有来源';
        case 'entity':
            return r.mentions !== undefined ? `出现过 ${r.mentions} 次` : r.summary;
        case 'relation':
            return r.confidence !== undefined
                ? `把握 ${Math.round(r.confidence * 100)}%`
                : r.summary;
        case 'fact':
            return '事实';
    }
}

/** 事实上游用撤回，不走删除 */
export const deletableKind = (
    k: MaiBotMemoryRecordKind,
): k is 'paragraph' | 'entity' | 'relation' => k !== 'fact';

export const RecordRow: React.FC<{
    record: MaiBotMemoryRecord;
    /** 只有按一种类型看时才给勾选：上游一次只删一种 */
    selectable: boolean;
    selected: boolean;
    busy: boolean;
    onPick: (shift: boolean) => void;
    onOpen: () => void;
    onDelete: () => void;
}> = ({ record: r, selectable, selected, busy, onPick, onOpen, onDelete }) => (
    <div
        className={cn(
            'group flex items-center gap-3 rounded-md border px-3 py-2.5 transition-colors',
            selected
                ? 'border-brand/40 bg-brand-soft/30'
                : 'border-border-subtle bg-surface hover:border-border',
        )}
    >
        {selectable && <RowCheck checked={selected} onPick={onPick} />}
        <KindIcon kind={r.kind} />
        <button
            type="button"
            onClick={onOpen}
            className="min-w-0 flex-1 text-left focus-visible:outline-none"
        >
            <span className="flex min-w-0 items-center gap-1.5">
                <span
                    className={cn(
                        'truncate text-[13.5px]',
                        r.active ? 'text-text' : 'text-text-tertiary line-through',
                    )}
                >
                    {r.title}
                </span>
                {!r.active && (
                    <Badge tone="neutral" className="shrink-0">
                        已删
                    </Badge>
                )}
            </span>
            <span className="mt-0.5 block truncate text-xs text-text-tertiary">
                {recordSubtitle(r)}
            </span>
        </button>
        <span className="hidden shrink-0 text-right text-2xs text-text-tertiary sm:block">
            {r.created_at ? relativeTime(r.created_at) : ''}
        </span>
        {deletableKind(r.kind) && r.active && (
            <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 shrink-0 text-danger opacity-0 transition-opacity hover:text-danger focus-visible:opacity-100 group-hover:opacity-100"
                aria-label="删除"
                disabled={busy}
                onClick={onDelete}
            >
                <Trash2 size={13} />
            </Button>
        )}
    </div>
);

const RelatedList: React.FC<{
    title: string;
    items: MaiBotMemoryRecord[];
    onOpen: (r: MaiBotMemoryRecord) => void;
}> = ({ title, items, onOpen }) =>
    items.length === 0 ? null : (
        <section className="flex flex-col gap-1.5">
            <h4 className="text-xs font-medium text-text-secondary">
                {title}
                <span className="ml-1.5 font-mono text-2xs text-text-tertiary">{items.length}</span>
            </h4>
            <div className="flex flex-col gap-1">
                {items.slice(0, 12).map((r) => (
                    <button
                        key={`${r.kind}:${r.id}`}
                        type="button"
                        onClick={() => onOpen(r)}
                        className="flex items-start gap-2 rounded-sm px-2 py-1.5 text-left text-xs text-text hover:bg-inset"
                    >
                        <KindIcon kind={r.kind} className="h-5 w-5 [&_svg]:h-3 [&_svg]:w-3" />
                        <span className="line-clamp-2 min-w-0 flex-1 leading-relaxed">
                            {r.kind === 'paragraph' ? r.summary : r.title}
                        </span>
                    </button>
                ))}
            </div>
        </section>
    );

/** 一条记忆的全貌：原文、谁撑着它、它牵连着什么；相关的点一下接着看 */
export const RecordDetailDialog: React.FC<{
    open: boolean;
    detail: MaiBotMemoryRecordDetail | undefined;
    loading: boolean;
    busy: boolean;
    onOpen: (r: MaiBotMemoryRecord) => void;
    onDelete: (r: MaiBotMemoryRecord) => void;
    onClose: () => void;
}> = ({ open, detail, loading, busy, onOpen, onDelete, onClose }) => {
    const r = detail?.record;
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="lg">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        {r && <KindIcon kind={r.kind} />}
                        <span className="min-w-0 truncate">
                            {r
                                ? r.kind === 'paragraph'
                                    ? `${KIND_LABEL.paragraph} · ${r.source || '没有来源'}`
                                    : r.title
                                : '记忆'}
                        </span>
                    </DialogTitle>
                </DialogHeader>
                {loading && !detail ? (
                    <div className="flex h-40 items-center justify-center">
                        <Spinner size="md" tone="brand" label="正在读取" />
                    </div>
                ) : r && detail ? (
                    <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto pr-1">
                        {r.kind === 'paragraph' && (
                            <p className="whitespace-pre-wrap rounded-md bg-field px-4 py-3 text-sm leading-relaxed text-text">
                                {r.summary}
                            </p>
                        )}
                        <p className="flex flex-wrap gap-x-3 gap-y-1 text-2xs text-text-tertiary">
                            <span>{KIND_LABEL[r.kind]}</span>
                            {r.mentions !== undefined && <span>出现在 {r.mentions} 段里</span>}
                            {r.confidence !== undefined && (
                                <span>把握 {Math.round(r.confidence * 100)}%</span>
                            )}
                            {r.created_at && <span>{relativeTime(r.created_at)}记下</span>}
                            {!r.active && <span className="text-danger">已删</span>}
                        </p>
                        <RelatedList
                            title="出自这些段落"
                            items={detail.paragraphs}
                            onOpen={onOpen}
                        />
                        <RelatedList title="提到的实体" items={detail.entities} onOpen={onOpen} />
                        <RelatedList title="相关的关系" items={detail.relations} onOpen={onOpen} />
                        <RelatedList title="相关的事实" items={detail.facts} onOpen={onOpen} />
                    </div>
                ) : (
                    <p className="py-8 text-center text-sm text-text-tertiary">这条记忆没找到</p>
                )}
                {r && deletableKind(r.kind) && r.active && (
                    <DialogFooter>
                        <Button
                            size="sm"
                            variant="ghost"
                            className="text-danger hover:bg-danger-soft hover:text-danger"
                            disabled={busy}
                            onClick={() => onDelete(r)}
                        >
                            <Trash2 size={13} />
                            删掉
                        </Button>
                    </DialogFooter>
                )}
            </DialogContent>
        </Dialog>
    );
};

/**
 * 删一批记忆的整套流程：先预览、再确认、执行完回调。浏览页和图谱页都用它，
 * 把返回的 `confirm` 放进页面里渲染
 */
export function useMemoryDelete(instanceId: string, onDone?: () => void) {
    const del = useMaiBotMemoryDelete(instanceId);
    const [pending, setPending] = useState<{
        target: MaiBotMemoryDeleteTarget;
        what: string;
    } | null>(null);
    const [preview, setPreview] = useState<MaiBotMemoryDeleteResult | undefined>();
    const start = (target: MaiBotMemoryDeleteTarget, what: string) => {
        setPending({ target, what });
        setPreview(undefined);
        del.mutateAsync({ op: 'preview', target }).then(setPreview, () => setPending(null));
    };
    const confirm = (
        <MemoryDeleteConfirm
            open={pending !== null}
            what={pending?.what ?? ''}
            preview={preview}
            previewing={del.isPending && !preview}
            busy={del.isPending && !!preview}
            onCancel={() => setPending(null)}
            onConfirm={() => {
                if (!pending) return;
                void del.mutateAsync({ op: 'execute', target: pending.target }).then(() => {
                    setPending(null);
                    onDone?.();
                });
            }}
        />
    );
    return { start, confirm, busy: del.isPending };
}

/** 「3 段、2 条关系」 */
export function describeCounts(c: MaiBotMemoryDeleteResult['counts']): string {
    const parts = [
        c.paragraphs > 0 && `${c.paragraphs} 段`,
        c.entities > 0 && `${c.entities} 个实体`,
        c.relations > 0 && `${c.relations} 条关系`,
    ].filter(Boolean);
    return parts.join('、') || '没有东西';
}

/**
 * 删之前先让上游算一遍：只靠这几段撑着的关系会一起没，这里把连带的也列出来。
 * 段落、实体是软删，一天左右内能在「最近删除」里恢复
 */
export const MemoryDeleteConfirm: React.FC<{
    open: boolean;
    what: string;
    preview: MaiBotMemoryDeleteResult | undefined;
    previewing: boolean;
    busy: boolean;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ open, what, preview, previewing, busy, onCancel, onConfirm }) => (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
        <DialogContent size="md" hideClose dismissOnOutsideClick={!busy}>
            <DialogHeader>
                <DialogTitle>删掉{what}？</DialogTitle>
                <DialogDescription>
                    {previewing || !preview
                        ? '正在算会连带删掉什么…'
                        : `会删掉 ${describeCounts(preview.counts)}。一天之内能在「最近删除」里恢复。`}
                </DialogDescription>
            </DialogHeader>
            {preview && preview.samples.length > 0 && (
                <ul className="mb-2 flex max-h-48 flex-col gap-1 overflow-y-auto rounded-md bg-field px-3 py-2">
                    {preview.samples.slice(0, 20).map((s, i) => (
                        <li key={i} className="flex gap-2 text-xs text-text-secondary">
                            <span className="shrink-0 text-text-tertiary">
                                {s.kind === 'paragraph'
                                    ? '段落'
                                    : s.kind === 'entity'
                                      ? '实体'
                                      : '关系'}
                            </span>
                            <span className="line-clamp-1 min-w-0">{s.preview || s.label}</span>
                        </li>
                    ))}
                </ul>
            )}
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
                    取消
                </Button>
                <Button
                    variant="danger"
                    size="sm"
                    onClick={onConfirm}
                    disabled={busy || previewing || !preview}
                >
                    {busy && <Spinner size="sm" className="text-white" />}
                    删掉
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
);
