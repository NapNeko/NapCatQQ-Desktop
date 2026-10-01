// 「对比响应」对话框：同一接口的两条历史记录，回包 JSON 并排，按行 diff 高亮。
// 左旧右新；绿色的行是新这份多出来的，红色的是旧这份有而新这份没的。
// 完整记录自己取（取过的有缓存）：面板只管把两条摘要传进来。

import { Fragment, useMemo } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, Spinner } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { diffAllSame, diffLines, type DiffRow } from '../../../core/domain/debug/linediff';
import { clockTimeMs, dayLabel, safeJson } from '../../../core/domain/debug/chatFormat';
import { useHistoryEntry } from '../../../hooks/debug/useDebugHistory';
import type { DebugHistoryEntry } from '../../../core/ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistorySummary } from '../../../core/ipc/generated/debug/DebugHistorySummary';

export interface HistoryDiffDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    action: string;
    /** 较早的 */
    left: DebugHistorySummary;
    /** 较新的 */
    right: DebugHistorySummary;
}

interface DiffSide {
    summary: DebugHistorySummary;
    /** 完整记录：undefined 还在取，null 是记录已经不在了 */
    entry: DebugHistoryEntry | null | undefined;
}

/** 回包的展示文本：没拿到回包的记录看错误本身，拿到了看原始回包 */
function entryText(entry: DebugHistoryEntry): string {
    return safeJson(entry.error ?? entry.response ?? null);
}

function SideHead({ label, side }: { label: string; side: DiffSide }) {
    const s = side.summary;
    const ok = s.ok;
    return (
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
            <span className="shrink-0 rounded-xs bg-inset px-1 py-px text-[10px] text-text-tertiary">{label}</span>
            <span className="min-w-0 truncate text-xs font-medium text-text">{s.bot_name}</span>
            <span className="shrink-0 text-2xs tabular-nums text-text-tertiary">
                {dayLabel(s.at_ms).replace(/ \d\d:\d\d$/, '')} {clockTimeMs(s.at_ms)}
            </span>
            <span className={cn('shrink-0 text-2xs', ok ? 'text-success' : 'text-danger')}>
                {ok ? '成功' : `失败${s.retcode !== null ? ` retcode ${s.retcode}` : ''}`}
            </span>
        </div>
    );
}

function SideBody({ side }: { side: DiffSide }) {
    if (side.entry === undefined) {
        return (
            <div className="flex items-center justify-center gap-2 py-10 text-2xs text-text-tertiary">
                <Spinner size="xs" label="正在取完整记录" />
                正在取完整记录…
            </div>
        );
    }
    if (side.entry === null) {
        return <p className="py-10 text-center text-2xs text-text-tertiary">这条记录已经不在了</p>;
    }
    return <pre className="px-2 py-1 font-mono text-[11px] leading-[18px] text-text">{entryText(side.entry)}</pre>;
}

/** 一格：both 是 same 的普通格；changed 是这一侧有改动的格（红 / 绿） */
function Cell({ text, tone }: { text: string | null; tone: 'same' | 'remove' | 'add' | 'empty' }) {
    return (
        <span
            className={cn(
                'min-w-0 whitespace-pre-wrap break-all px-2 py-0 font-mono text-[11px] leading-[18px]',
                tone === 'remove' && 'bg-danger-soft/70',
                tone === 'add' && 'bg-success-soft/70',
                tone === 'empty' && 'bg-inset/40',
            )}
        >
            {text ?? ''}
        </span>
    );
}

function DiffGrid({ left, right }: { left: string; right: string }) {
    const rows = useMemo(() => diffLines(left, right), [left, right]);
    return (
        <>
            {diffAllSame(rows) && <p className="px-2 py-1 text-2xs text-text-tertiary">两份回包完全一致</p>}
            <div className="grid grid-cols-2 gap-x-px">
                {rows.map((r: DiffRow, i) => (
                    <Fragment key={i}>
                        <Cell text={r.left} tone={r.kind === 'remove' ? 'remove' : r.kind === 'add' ? 'empty' : 'same'} />
                        <Cell text={r.right} tone={r.kind === 'add' ? 'add' : r.kind === 'remove' ? 'empty' : 'same'} />
                    </Fragment>
                ))}
            </div>
        </>
    );
}

export function HistoryDiffDialog({ open, onOpenChange, action, left, right }: HistoryDiffDialogProps) {
    const leftQuery = useHistoryEntry(left.id);
    const rightQuery = useHistoryEntry(right.id);
    const sides: { left: DiffSide; right: DiffSide } = {
        left: { summary: left, entry: leftQuery.data },
        right: { summary: right, entry: rightQuery.data },
    };
    const ready = sides.left.entry != null && sides.right.entry != null;
    const truncated = (sides.left.entry?.response_truncated ?? false) || (sides.right.entry?.response_truncated ?? false);
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sheetWide">
                <DialogHeader>
                    <DialogTitle>
                        对比响应 · <code className="font-mono">{action}</code>
                    </DialogTitle>
                </DialogHeader>
                <div className="grid shrink-0 grid-cols-2 gap-x-2">
                    <SideHead label="较旧" side={sides.left} />
                    <SideHead label="较新" side={sides.right} />
                </div>
                {truncated && <p className="shrink-0 text-2xs text-warning">有记录的回包太大被截断过，对比的是留下来的部分</p>}
                <div className="min-h-0 flex-1 overflow-auto rounded-sm border border-border-subtle bg-surface">
                    {ready ? (
                        <DiffGrid left={entryText(sides.left.entry!)} right={entryText(sides.right.entry!)} />
                    ) : (
                        <div className="grid grid-cols-2 gap-x-px">
                            <SideBody side={sides.left} />
                            <SideBody side={sides.right} />
                        </div>
                    )}
                </div>
            </DialogContent>
        </Dialog>
    );
}
