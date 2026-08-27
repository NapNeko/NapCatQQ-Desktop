// 设置 · 日志 Tab：grid 行布局（与 Bot 日志页同一套行高/列宽策略与右键复制交互）。

import { useState, type Ref, type RefObject } from 'react';
import { Copy, ScrollText } from 'lucide-react';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuTrigger,
} from '../../../shared/ui';
import { serializeLogs, type LogEntry } from '../../../core/domain/events/log-buffer';
import {
    LOG_LEVEL_SHORT,
    levelBarColor,
    levelLabelColor,
    lineTextColor,
} from '../../../shared/log/log-level-display';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { cn } from '../../../shared/utils/cn';

const LOG_SURFACE =
    'bg-[color-mix(in_srgb,var(--surface-canvas)_76%,var(--surface-inset)_24%)]';

/** 列表里只显示时分秒，完整时间在 title；避免宽时间列留白造成「和 INFO 隔很远」。 */
function displayTime(timestamp: string): string {
    const t = timestamp.trim();
    const m = t.match(/(\d{2}:\d{2}:\d{2})\s*$/);
    return m ? m[1] : t;
}

type Props = {
    emptyKind: 'loading' | 'error' | 'empty-file' | 'no-match' | 'has';
    entries: LogEntry[];
    fontSize: number;
    viewportRef: RefObject<HTMLDivElement | null>;
    error: string | null;
};

export function DesktopLogTab({ emptyKind, entries, fontSize, viewportRef, error }: Props) {
    const rowPx = Math.max(20, Math.round(fontSize * 1.5));
    const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
    const [lastClickedIndex, setLastClickedIndex] = useState<number | null>(null);
    const [contextEntry, setContextEntry] = useState<LogEntry | null>(null);

    const handleRowClick = (index: number, entry: LogEntry, e: React.MouseEvent) => {
        if (e.shiftKey && lastClickedIndex !== null) {
            const start = Math.min(lastClickedIndex, index);
            const end = Math.max(lastClickedIndex, index);
            const newSet = new Set<string>();
            for (let i = start; i <= end; i++) {
                const item = entries[i];
                if (item) newSet.add(item.id);
            }
            setSelectedIds(newSet);
        } else if (e.ctrlKey || e.metaKey) {
            setSelectedIds((prev) => {
                const next = new Set(prev);
                if (next.has(entry.id)) {
                    next.delete(entry.id);
                } else {
                    next.add(entry.id);
                }
                return next;
            });
            setLastClickedIndex(index);
        } else {
            setSelectedIds(new Set([entry.id]));
            setLastClickedIndex(index);
        }
        setContextEntry(entry);
    };

    const handleRowContextMenu = (index: number, entry: LogEntry) => {
        setContextEntry(entry);
        if (!selectedIds.has(entry.id)) {
            setSelectedIds(new Set([entry.id]));
            setLastClickedIndex(index);
        }
    };

    const onCopySelected = async () => {
        const selectedLogs = entries.filter((l) => selectedIds.has(l.id));
        if (selectedLogs.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(selectedLogs));
            pushInfoBar({
                tone: 'info',
                title: '已复制选中日志',
                content: `共复制 ${selectedLogs.length} 行控制台日志`,
                autoDismissMs: 2000,
            });
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('复制日志失败:', err);
        }
    };

    const onCopyCurrentLine = async () => {
        const target = contextEntry || entries[0];
        if (!target) return;
        try {
            await navigator.clipboard.writeText(serializeLogs([target]));
            pushInfoBar({
                tone: 'info',
                title: '已复制单行日志',
                content: target.text,
                autoDismissMs: 2000,
            });
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('复制日志失败:', err);
        }
    };

    const onCopyAll = async () => {
        if (entries.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(entries));
            pushInfoBar({
                tone: 'info',
                title: '已复制全部日志',
                content: `共复制 ${entries.length} 条控制台日志记录`,
                autoDismissMs: 2000,
            });
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('复制日志失败:', err);
        }
    };

    return (
        <div
            className={`flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle/50 ${LOG_SURFACE}`}
        >
            {emptyKind !== 'has' ? (
                <LogEmptyState kind={emptyKind} message={error ?? undefined} />
            ) : (
                <ContextMenu>
                    <ContextMenuTrigger asChild>
                        <div
                            ref={viewportRef as Ref<HTMLDivElement>}
                            role="log"
                            aria-live="off"
                            aria-label="桌面端调试日志"
                            className="scrollbar-hide min-h-0 flex-1 overflow-auto bg-inset/30 px-4 py-4 font-mono select-none"
                            style={{
                                fontSize: `${fontSize}px`,
                                lineHeight: `${rowPx}px`,
                                userSelect: 'none',
                                WebkitUserSelect: 'none',
                            }}
                        >
                            {entries.map((e, idx) => (
                                <DesktopLogLine
                                    key={e.id}
                                    entry={e}
                                    rowPx={rowPx}
                                    selected={selectedIds.has(e.id)}
                                    onClick={(evt) => handleRowClick(idx, e, evt)}
                                    onContextMenu={() => handleRowContextMenu(idx, e)}
                                />
                            ))}
                        </div>
                    </ContextMenuTrigger>
                    <ContextMenuContent className="w-56">
                        {selectedIds.size > 1 ? (
                            <ContextMenuItem
                                onClick={onCopySelected}
                                className="flex items-center gap-2"
                            >
                                <Copy size={13} className="text-brand" />
                                <span>复制选中日志</span>
                                <span className="ml-auto text-2xs text-text-tertiary">
                                    {selectedIds.size} 行
                                </span>
                            </ContextMenuItem>
                        ) : (
                            <ContextMenuItem
                                onClick={onCopyCurrentLine}
                                disabled={!contextEntry && entries.length === 0}
                                className="flex items-center gap-2"
                            >
                                <Copy size={13} className="text-text-tertiary" />
                                <span>复制当前行</span>
                            </ContextMenuItem>
                        )}
                        <ContextMenuItem onClick={onCopyAll} disabled={entries.length === 0} className="flex items-center gap-2">
                            <Copy size={13} className="text-text-tertiary" />
                            <span>复制全部日志</span>
                            <span className="ml-auto text-2xs text-text-tertiary">
                                {entries.length} 行
                            </span>
                        </ContextMenuItem>
                    </ContextMenuContent>
                </ContextMenu>
            )}
        </div>
    );
}

function DesktopLogLine({
    entry,
    rowPx,
    selected,
    onClick,
    onContextMenu,
}: {
    entry: LogEntry;
    rowPx: number;
    selected: boolean;
    onClick: (e: React.MouseEvent) => void;
    onContextMenu: () => void;
}) {
    const level = entry.level;
    const label = desktopLevelLabel(entry);
    const timeShort = displayTime(entry.timestamp);
    const timeFull = entry.timestamp || '—';

    return (
        <div
            onMouseDown={(e) => {
                // 彻底阻止浏览器原生 Shift+Click 与鼠标拖选产生粗暴蓝色文字遮罩
                e.preventDefault();
                window.getSelection()?.removeAllRanges();
            }}
            onClick={onClick}
            onContextMenu={onContextMenu}
            className={cn(
                'group grid items-center gap-x-1.5 px-1 cursor-pointer select-none transition-colors',
                selected ? 'bg-brand-soft/40 ring-1 ring-inset ring-brand/30' : 'hover:bg-elevated/80',
            )}
            style={{
                height: rowPx,
                gridTemplateColumns: '3px 4.5rem 1.75rem minmax(0, 1fr)',
                userSelect: 'none',
                WebkitUserSelect: 'none',
            }}
        >
            <span
                className="h-3 w-[3px] shrink-0 justify-self-center"
                style={{ background: levelBarColor(level) }}
            />
            <span
                className="truncate text-text-tertiary"
                style={{ fontSize: Math.max(10, rowPx - 7) }}
                title={timeFull}
            >
                {timeShort || '—'}
            </span>
            <span
                className="font-semibold uppercase"
                style={{
                    fontSize: Math.max(9, rowPx - 8),
                    color: levelLabelColor(level),
                }}
            >
                {label}
            </span>
            <span
                className="truncate"
                style={{ color: lineTextColor(level) }}
            >
                {entry.text || ' '}
            </span>
        </div>
    );
}

function desktopLevelLabel(entry: LogEntry): string {
    if (entry.levelTag) {
        const inner = entry.levelTag.replace(/^\[|\]$/g, '').trim().toUpperCase();
        if (inner === 'INFO') return 'INF';
        if (inner.length <= 3) return inner;
        return inner.slice(0, 3);
    }
    return LOG_LEVEL_SHORT[entry.level];
}

function LogEmptyState({
    kind,
    message,
}: {
    kind: 'loading' | 'error' | 'empty-file' | 'no-match';
    message?: string;
}) {
    const copy =
        kind === 'loading'
            ? { title: '正在加载', body: '读取当前会话日志文件…' }
            : kind === 'error'
                ? { title: '加载失败', body: message ?? '无法读取日志文件' }
                : kind === 'empty-file'
                    ? { title: '暂无内容', body: '当前日志文件为空' }
                    : { title: '没有匹配的行', body: '试试改下搜索关键字或切换等级' };

    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2.5 py-20 text-center">
            <ScrollText size={20} strokeWidth={1.5} className="text-text-tertiary/60" />
            <p className="text-[13px] font-medium text-text-secondary">{copy.title}</p>
            <p className="max-w-xs text-[12px] leading-relaxed text-text-tertiary">{copy.body}</p>
        </div>
    );
}