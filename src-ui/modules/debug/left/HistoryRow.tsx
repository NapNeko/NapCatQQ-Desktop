// 调用历史里的一行：成败图标、接口名、耗时；第二行 Bot 名和多久以前（失败时带 retcode 或原因）。
// 悬停出现「收藏」「复制参数」，右键菜单里也有。对比模式下最左边多一个勾选框，单击切换勾选而不是打开。

import { memo } from 'react';
import { Check, CheckCircle2, Copy, ExternalLink, Star, XCircle } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    ContextMenuTrigger,
    Spinner,
} from '../../../shared/ui';
import { relativeTimeFromMs } from '../../../core/domain/ui/relativeTime';
import { channelShortLabel } from '../../../core/domain/debug/channelCopy';
import type { DebugHistorySummary } from '../../../core/ipc/generated/debug/DebugHistorySummary';
import { errorKindTitle } from '../../../core/domain/debug/historyReplay';
import { IconAction } from './panelParts';

export const HISTORY_ROW_HEIGHT = 46;

export type HistoryRowIntent = 'open' | 'save' | 'copy';

export interface HistoryRowProps {
    id: string;
    entry: DebugHistorySummary;
    nowMs: number;
    active: boolean;
    /** 这一行正在取完整记录（打开 / 收藏 / 复制前） */
    busy: boolean;
    /** 对比模式的勾选态；不在这个模式是 null（单击照常在新标签打开） */
    compare: { selected: boolean; onToggle: () => void } | null;
    onIntent: (entry: DebugHistorySummary, intent: HistoryRowIntent) => void;
    onCopyAction: (name: string) => void;
}

function failureNote(entry: DebugHistorySummary): string | null {
    if (entry.ok) return null;
    if (entry.retcode !== null) return `retcode ${entry.retcode}`;
    return errorKindTitle(entry.error_kind) ?? '失败';
}

function formatElapsed(ms: number): string {
    return ms >= 10_000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
}

export const HistoryRow = memo(function HistoryRow({
    id,
    entry,
    nowMs,
    active,
    busy,
    compare,
    onIntent,
    onCopyAction,
}: HistoryRowProps) {
    const when = relativeTimeFromMs(entry.at_ms, undefined, nowMs) ?? '';
    const failure = failureNote(entry);
    // 全局的 title 气泡是单行的，用「·」隔开
    const title = [
        entry.bot_name,
        channelShortLabel(entry.channel),
        new Date(entry.at_ms).toLocaleString(),
        entry.ok ? `成功 ${formatElapsed(entry.elapsed_ms)}` : `失败：${failure}`,
        compare ? '单击勾选 / 取消勾选' : '单击在新标签打开（带着当时的响应）',
    ].join(' · ');

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild>
                <div
                    id={id}
                    role="option"
                    aria-selected={compare ? compare.selected : active}
                    aria-busy={busy || undefined}
                    title={title}
                    onClick={() => (compare ? compare.onToggle() : onIntent(entry, 'open'))}
                    onMouseDown={(e) => {
                        if (e.button === 1) e.preventDefault();
                    }}
                    onAuxClick={(e) => {
                        if (e.button === 1 && !compare) onIntent(entry, 'open');
                    }}
                    style={{ height: HISTORY_ROW_HEIGHT }}
                    className={cn(
                        'group relative flex cursor-pointer select-none items-center gap-2 rounded-sm px-2 transition-colors hover:bg-elevated/35',
                        active && 'bg-inset ring-1 ring-inset ring-brand/50',
                        compare?.selected && 'bg-brand-soft/50',
                    )}
                >
                    {compare && (
                        <span
                            aria-hidden
                            className={cn(
                                'flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-xs border transition-colors',
                                compare.selected
                                    ? 'border-brand bg-brand text-white'
                                    : 'border-border bg-field',
                            )}
                        >
                            {compare.selected && <Check size={11} strokeWidth={3} />}
                        </span>
                    )}
                    <span className="flex h-4 w-4 shrink-0 items-center justify-center">
                        {busy ? (
                            <Spinner size="xs" label="正在读取这条记录" />
                        ) : entry.ok ? (
                            <CheckCircle2
                                size={14}
                                strokeWidth={2.2}
                                aria-label="成功"
                                className="text-success"
                            />
                        ) : (
                            <XCircle
                                size={14}
                                strokeWidth={2.2}
                                aria-label="失败"
                                className="text-danger"
                            />
                        )}
                    </span>
                    <span className="min-w-0 flex-1">
                        <span className="flex min-w-0 items-center gap-2">
                            <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-text">
                                {entry.action}
                            </span>
                            <span className="shrink-0 text-[10px] tabular-nums text-text-tertiary transition-opacity group-focus-within:opacity-0 group-hover:opacity-0">
                                {formatElapsed(entry.elapsed_ms)}
                            </span>
                        </span>
                        <span className="mt-0.5 flex min-w-0 items-center gap-1 text-[11px] text-text-tertiary">
                            <span className="min-w-0 truncate">{entry.bot_name}</span>
                            <span aria-hidden>·</span>
                            <span className="shrink-0">{when}</span>
                            {failure && (
                                <>
                                    <span aria-hidden>·</span>
                                    <span className="min-w-0 truncate text-danger">{failure}</span>
                                </>
                            )}
                        </span>
                    </span>
                    <span
                        className={cn(
                            'absolute right-1.5 top-1 flex items-center gap-0.5 rounded-sm bg-surface/90 opacity-0 transition-opacity',
                            'pointer-events-none group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100',
                        )}
                    >
                        <IconAction
                            icon={Star}
                            label="收藏"
                            tone="brand"
                            focusable={false}
                            onClick={() => onIntent(entry, 'save')}
                        />
                        <IconAction
                            icon={Copy}
                            label="复制参数"
                            focusable={false}
                            onClick={() => onIntent(entry, 'copy')}
                        />
                    </span>
                </div>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-48">
                <ContextMenuLabel className="truncate font-mono text-2xs">
                    {entry.action}
                </ContextMenuLabel>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => onIntent(entry, 'open')}>
                    <ExternalLink size={13} />
                    <span>在新标签打开</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onIntent(entry, 'save')}>
                    <Star size={13} />
                    <span>收藏</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onIntent(entry, 'copy')}>
                    <Copy size={13} />
                    <span>复制参数</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onCopyAction(entry.action)}>
                    <Copy size={13} />
                    <span>复制接口名</span>
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
});
