// 完整消息构建器：把当前草稿（构建器段 + 手打文字）解析成段列表，
// 加点、删段、上移下移、按类型填字段，底部实时预览将要发出的消息段。
// 「使用这些段」写回同一份草稿（全是文字时折叠回输入框，否则留在构建器段里），输入框照常 Enter 发送。
//
// 段校验只做提示不挡「使用」——调试台有时候就是要发畸形消息看上游反应。

import { useMemo, useRef, useState } from 'react';
import { Blocks, ChevronDown, ChevronUp, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
} from '../../../shared/ui';
import { messagePreview, segmentPreview, type Segment } from '../../../core/domain/debug/segments';
import {
    BUILDER_KINDS,
    blankSegment,
    builderKindLabel,
    normalizeReplies,
    parseEntryToSegments,
    segmentIssue,
    segmentsToDraft,
    type ComposerEntry,
} from '../../../core/domain/debug/messageBuilder';
import { IconAction } from './rightParts';

// 段再多就只会是误操作了（反复「解析现有内容」也会指数翻倍），给个上限
const MAX_SEGMENTS = 50;

export interface MessageBuilderProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 打开那一刻的草稿；对话框是模态的，期间输入框不会变 */
    entry: ComposerEntry;
    /** 输入框上挂着的回复（点气泡来的），预览里一起算上 */
    replyId?: number | null;
    onApply: (next: ComposerEntry) => void;
}

export function MessageBuilderDialog(props: MessageBuilderProps) {
    // 每次打开重新挂载：段列表按当时的草稿重新解析；关的动画期间内容还在，收起时不闪空
    const [mounted, setMounted] = useState(props.open);
    if (props.open && !mounted) setMounted(true);
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            <DialogContent
                size="lg"
                dismissOnOutsideClick={false}
                onExited={() => setMounted(false)}
            >
                {mounted && <BuilderBody {...props} />}
            </DialogContent>
        </Dialog>
    );
}

/** 列表行：key 稳定，上移下移时字段焦点不跳 */
interface Row {
    key: string;
    seg: Segment;
}

function BuilderBody({ onOpenChange, entry, replyId, onApply }: MessageBuilderProps) {
    const keySeq = useRef(0);
    const nextKey = () => `seg-${keySeq.current++}`;
    const [rows, setRows] = useState<Row[]>(() =>
        parseEntryToSegments(entry).map((seg) => ({ key: nextKey(), seg })),
    );

    const mutate = (key: string, data: Record<string, unknown>) =>
        setRows((list) =>
            list.map((r) =>
                r.key === key ? { ...r, seg: { ...r.seg, data: { ...r.seg.data, ...data } } } : r,
            ),
        );
    const move = (index: number, delta: -1 | 1) =>
        setRows((list) => {
            const to = index + delta;
            if (to < 0 || to >= list.length) return list;
            const next = [...list];
            const [row] = next.splice(index, 1);
            next.splice(to, 0, row as Row);
            return next;
        });
    const remove = (key: string) => setRows((list) => list.filter((r) => r.key !== key));
    const append = (type: string) => {
        if (rows.length >= MAX_SEGMENTS) return;
        setRows((list) => [...list, { key: nextKey(), seg: blankSegment(type) }]);
    };
    const reset = () =>
        setRows(parseEntryToSegments(entry).map((seg) => ({ key: nextKey(), seg })));

    const segments = useMemo(() => rows.map((r) => r.seg), [rows]);
    const preview = useMemo(() => normalizeReplies(segments, replyId), [segments, replyId]);

    const apply = () => {
        // 写回前把回复段归一到最前，之后摘要行和发出的消息同一个顺序
        const list = normalizeReplies(segments);
        const back = segmentsToDraft(list, entry.mentions);
        onApply(back ? { ...back, rich: [] } : { text: '', mentions: [], rich: list });
        onOpenChange(false);
    };

    return (
        <form
            onSubmit={(e) => {
                e.preventDefault();
                apply();
            }}
        >
            <DialogHeader>
                <DialogTitle>消息构建器</DialogTitle>
                <DialogDescription>
                    拼装要发送的消息段。确定后写回输入框草稿，Enter 照常发送。
                </DialogDescription>
            </DialogHeader>

            <div className="flex max-h-[300px] min-h-[120px] flex-col gap-1.5 overflow-y-auto rounded-md border border-border-subtle/60 bg-inset/30 p-1.5">
                {rows.length === 0 && (
                    <p className="px-1.5 py-3 text-center text-2xs text-text-tertiary">
                        还没有段，在下面选一种加上。
                    </p>
                )}
                {rows.map((row, i) => (
                    <SegmentRow
                        key={row.key}
                        row={row}
                        first={i === 0}
                        last={i === rows.length - 1}
                        onChange={(data) => mutate(row.key, data)}
                        onMove={(delta) => move(i, delta)}
                        onRemove={() => remove(row.key)}
                    />
                ))}
            </div>

            <div className="mt-2 flex items-center gap-2">
                <span className="flex items-center gap-1 text-2xs text-text-tertiary">
                    <Blocks size={11} aria-hidden />
                    {rows.length > 0 ? `${rows.length} 段` : '添加段'}
                </span>
                <div className="w-44">
                    <Select
                        placeholder="选择类型…"
                        value=""
                        onValueChange={append}
                        disabled={rows.length >= MAX_SEGMENTS}
                        items={BUILDER_KINDS.map((k) => ({ value: k.type, label: k.label }))}
                    />
                </div>
            </div>

            <div className="mt-3 rounded-md bg-inset/50 px-3 py-2">
                <p className="text-2xs font-medium text-text-tertiary">
                    发送预览{replyId ? '（含回复）' : ''}
                </p>
                <p className="mt-0.5 break-words text-[13px] leading-5 text-text">
                    {messagePreview(preview) || '（空消息）'}
                </p>
                <pre
                    aria-label="消息段 JSON"
                    className="mt-1.5 max-h-[120px] overflow-auto whitespace-pre-wrap break-all rounded-xs bg-canvas/70 p-2 font-mono text-[11px] leading-relaxed text-text-secondary"
                >
                    {JSON.stringify(preview, null, 1)}
                </pre>
            </div>

            <DialogFooter>
                <Button type="button" variant="ghost" size="sm" className="mr-auto" onClick={reset}>
                    解析现有内容
                </Button>
                <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
                    取消
                </Button>
                <Button type="submit" variant="primary" size="sm">
                    使用这些段
                </Button>
            </DialogFooter>
        </form>
    );
}

// ---------------------------------------------------------------------------
// 一行段
// ---------------------------------------------------------------------------

const inputCls =
    'h-7 w-full rounded-sm border border-border-subtle bg-field px-2 text-xs text-text outline-none transition-colors placeholder:text-text-tertiary focus:border-brand';
const areaCls = cn(inputCls, 'h-auto min-h-[48px] resize-y py-1.5');

function SegmentRow({
    row,
    first,
    last,
    onChange,
    onMove,
    onRemove,
}: {
    row: Row;
    first: boolean;
    last: boolean;
    onChange: (data: Record<string, unknown>) => void;
    onMove: (delta: -1 | 1) => void;
    onRemove: () => void;
}) {
    const { seg } = row;
    const issue = segmentIssue(seg);
    return (
        <div className="rounded-md border border-border-subtle/70 bg-surface px-2 py-1.5">
            <div className="flex items-center gap-1.5">
                <span className="shrink-0 rounded-xs bg-brand-soft px-1.5 py-px text-2xs font-medium text-brand">
                    {builderKindLabel(seg.type)}
                </span>
                <span className="min-w-0 flex-1 truncate text-2xs text-text-tertiary">
                    {segmentPreview(seg)}
                </span>
                <IconAction
                    label="上移"
                    tip="上移"
                    disabled={first}
                    onClick={() => onMove(-1)}
                    className="h-5 w-5"
                >
                    <ChevronUp size={12} aria-hidden />
                </IconAction>
                <IconAction
                    label="下移"
                    tip="下移"
                    disabled={last}
                    onClick={() => onMove(1)}
                    className="h-5 w-5"
                >
                    <ChevronDown size={12} aria-hidden />
                </IconAction>
                <IconAction
                    label="删掉这段"
                    tip="删掉这段"
                    tone="danger"
                    onClick={onRemove}
                    className="h-5 w-5"
                >
                    <X size={12} aria-hidden />
                </IconAction>
            </div>
            <div className="mt-1.5 flex flex-col gap-1.5">
                <SegmentFields seg={seg} onChange={onChange} />
            </div>
            {issue && <p className="mt-1 text-2xs text-warning">{issue}</p>}
        </div>
    );
}

function SegmentFields({
    seg,
    onChange,
}: {
    seg: Segment;
    onChange: (data: Record<string, unknown>) => void;
}) {
    const d = seg.data;
    const s = (v: unknown) => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '');
    switch (seg.type) {
        case 'text':
            return (
                <textarea
                    aria-label="文字内容"
                    rows={2}
                    value={s(d.text)}
                    onChange={(e) => onChange({ text: e.target.value })}
                    className={areaCls}
                />
            );
        case 'at': {
            const isAll = s(d.qq) === 'all';
            return (
                <div className="flex items-center gap-2">
                    <input
                        aria-label="QQ 号"
                        placeholder="QQ 号"
                        inputMode="numeric"
                        value={isAll ? '' : s(d.qq)}
                        disabled={isAll}
                        onChange={(e) => onChange({ qq: e.target.value.trim() })}
                        className={cn(inputCls, 'flex-1')}
                    />
                    <label className="flex shrink-0 items-center gap-1 text-2xs text-text-secondary">
                        <input
                            type="checkbox"
                            checked={isAll}
                            onChange={(e) => onChange({ qq: e.target.checked ? 'all' : '' })}
                            className="h-3.5 w-3.5 accent-brand"
                        />
                        全体成员
                    </label>
                </div>
            );
        }
        case 'face':
            return (
                <input
                    aria-label="表情 id"
                    placeholder="表情 id"
                    inputMode="numeric"
                    value={s(d.id)}
                    onChange={(e) => onChange({ id: e.target.value.trim() })}
                    className={inputCls}
                />
            );
        case 'image':
            return (
                <>
                    <input
                        aria-label="图片地址"
                        placeholder="URL / base64:// / 本地路径"
                        value={s(d.file)}
                        onChange={(e) => onChange({ file: e.target.value })}
                        className={inputCls}
                    />
                    <input
                        aria-label="图片说明"
                        placeholder="说明（可空）"
                        value={s(d.summary)}
                        onChange={(e) => onChange({ summary: e.target.value })}
                        className={inputCls}
                    />
                </>
            );
        case 'record':
        case 'video':
            return (
                <input
                    aria-label={seg.type === 'record' ? '语音地址' : '视频地址'}
                    placeholder="URL / base64:// / 本地路径"
                    value={s(d.file)}
                    onChange={(e) => onChange({ file: e.target.value })}
                    className={inputCls}
                />
            );
        case 'reply':
            return (
                <input
                    aria-label="回复的消息 id"
                    placeholder="消息 id"
                    inputMode="numeric"
                    value={s(d.id)}
                    onChange={(e) => onChange({ id: e.target.value.trim() })}
                    className={inputCls}
                />
            );
        case 'poke':
            return (
                <div className="flex gap-1.5">
                    <input
                        aria-label="戳一戳类型"
                        placeholder="类型"
                        inputMode="numeric"
                        value={s(d.type)}
                        onChange={(e) => onChange({ type: e.target.value.trim() })}
                        className={inputCls}
                    />
                    <input
                        aria-label="戳一戳 id"
                        placeholder="id"
                        inputMode="numeric"
                        value={s(d.id)}
                        onChange={(e) => onChange({ id: e.target.value.trim() })}
                        className={inputCls}
                    />
                </div>
            );
        case 'markdown':
            return (
                <textarea
                    aria-label="Markdown 内容"
                    rows={3}
                    value={s(d.content)}
                    onChange={(e) => onChange({ content: e.target.value })}
                    className={areaCls}
                />
            );
        case 'json':
        case 'xml':
            return (
                <textarea
                    aria-label={seg.type === 'json' ? '卡片 JSON' : '卡片 XML'}
                    rows={3}
                    value={s(d.data)}
                    onChange={(e) => onChange({ data: e.target.value })}
                    className={cn(areaCls, 'font-mono text-[11px]')}
                />
            );
        default:
            return <p className="text-2xs text-text-tertiary">这种段没有可填的字段，原样发送。</p>;
    }
}
