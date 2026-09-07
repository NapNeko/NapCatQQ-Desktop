// Bot 日志页抽出来的控制台：搜索 / 级别 / 复制 / 虚拟列表。应用端日志 Tab 共用。

import { useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Brush, Copy, Filter, Search, ScrollText, Pause, Play } from 'lucide-react';
import {
    Button,
    ContextMenu,
    ContextMenuTrigger,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuSub,
    ContextMenuSubTrigger,
    ContextMenuSubContent,
    ContextMenuRadioGroup,
    ContextMenuRadioItem,
} from '../ui';
import { ActionMotionIcon, LIVE_MOTION } from '../ui/motion';
import {
    filterLogs,
    serializeLogs,
    type LevelFilter,
    type LogEntry,
} from '../../core/domain/events/log-buffer';
import { LOG_LEVEL_SHORT, levelBarColor, levelLabelColor, lineTextColor } from './log-level-display';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { cn } from '../utils/cn';

const LEVEL_LABEL = LOG_LEVEL_SHORT;
const LOG_ROW_HEIGHT_PX = 20;

const LEVEL_FILTERS: { value: LevelFilter; label: string }[] = [
    { value: 'all', label: '全部' },
    { value: 'info', label: '信息' },
    { value: 'warn', label: '警告' },
    { value: 'error', label: '错误' },
    { value: 'debug', label: '调试' },
];

export interface LogConsoleProps {
    logs: LogEntry[];
    onClear: () => void;
    emptyTitle?: string;
    emptyBody?: string;
    ariaLabel?: string;
}

export function LogConsole({
    logs,
    onClear,
    emptyTitle = '暂无日志',
    emptyBody = '实例可能尚未启动，或当前还没触发任何输出',
    ariaLabel = '实例运行日志',
}: LogConsoleProps) {
    const [query, setQuery] = useState('');
    const [levelFilter, setLevelFilter] = useState<LevelFilter>('all');
    const [autoScroll, setAutoScroll] = useState(true);
    const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
    const [lastClickedIndex, setLastClickedIndex] = useState<number | null>(null);
    const [contextEntry, setContextEntry] = useState<LogEntry | null>(null);

    const filtered = useMemo(
        () => filterLogs(logs, query, 'all', levelFilter),
        [logs, query, levelFilter],
    );

    const handleRowClick = (index: number, entry: LogEntry, e: React.MouseEvent) => {
        if (e.shiftKey && lastClickedIndex !== null) {
            const start = Math.min(lastClickedIndex, index);
            const end = Math.max(lastClickedIndex, index);
            const newSet = new Set<string>();
            for (let i = start; i <= end; i++) {
                const item = filtered[i];
                if (item) newSet.add(item.id);
            }
            setSelectedIds(newSet);
        } else if (e.ctrlKey || e.metaKey) {
            setSelectedIds((prev) => {
                const next = new Set(prev);
                if (next.has(entry.id)) next.delete(entry.id);
                else next.add(entry.id);
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
        const selectedLogs = filtered.filter((l) => selectedIds.has(l.id));
        if (selectedLogs.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(selectedLogs));
            pushInfoBar({
                tone: 'info',
                title: '已复制选中日志',
                content: `共复制 ${selectedLogs.length} 行日志`,
                autoDismissMs: 2000,
            });
        } catch (err) {
            console.warn('复制日志失败:', err);
        }
    };

    const onCopyCurrentLine = async () => {
        const target = contextEntry || filtered[0];
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
            console.warn('复制日志失败:', err);
        }
    };

    const onCopyAll = async () => {
        if (filtered.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(filtered));
            pushInfoBar({
                tone: 'info',
                title: '已复制全部日志',
                content: `共复制 ${filtered.length} 条日志记录`,
                autoDismissMs: 2000,
            });
        } catch (err) {
            console.warn('复制日志失败:', err);
        }
    };

    const emptyKind: 'no-logs' | 'no-match' | 'has' =
        logs.length === 0 ? 'no-logs' : filtered.length === 0 ? 'no-match' : 'has';

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild>
                <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md ring-1 ring-border-subtle">
                    <Toolbar
                        query={query}
                        onQuery={setQuery}
                        levelFilter={levelFilter}
                        onLevelFilter={setLevelFilter}
                        autoScroll={autoScroll}
                        onToggleAutoScroll={() => setAutoScroll((p) => !p)}
                        onClear={onClear}
                        onCopy={selectedIds.size > 1 ? onCopySelected : onCopyAll}
                        hasLogs={logs.length > 0}
                        hasVisible={filtered.length > 0}
                    />
                    <LogViewport
                        entries={filtered}
                        emptyKind={emptyKind}
                        emptyTitle={emptyTitle}
                        emptyBody={emptyBody}
                        ariaLabel={ariaLabel}
                        autoScroll={autoScroll}
                        selectedIds={selectedIds}
                        onRowClick={handleRowClick}
                        onRowContextMenu={handleRowContextMenu}
                    />
                </div>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-52">
                {selectedIds.size > 1 ? (
                    <ContextMenuItem onClick={onCopySelected}>
                        <Copy size={13} className="text-brand" />
                        <span>复制选中日志 ({selectedIds.size} 行)</span>
                    </ContextMenuItem>
                ) : (
                    <ContextMenuItem
                        onClick={onCopyCurrentLine}
                        disabled={!contextEntry && filtered.length === 0}
                    >
                        <Copy size={13} />
                        <span>复制当前行日志</span>
                    </ContextMenuItem>
                )}
                <ContextMenuItem onClick={onCopyAll} disabled={filtered.length === 0}>
                    <Copy size={13} />
                    <span>复制全部日志 ({filtered.length} 行)</span>
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => setAutoScroll((p) => !p)}>
                    {autoScroll ? <Pause size={13} /> : <Play size={13} />}
                    <span>{autoScroll ? '暂停自动滚动' : '开启自动滚动'}</span>
                </ContextMenuItem>
                <ContextMenuItem tone="danger" onClick={onClear} disabled={logs.length === 0}>
                    <Brush size={13} />
                    <span>清空当前日志</span>
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuSub>
                    <ContextMenuSubTrigger>
                        <Filter size={13} className="mr-2 text-text-secondary" />
                        <span>日志等级过滤</span>
                    </ContextMenuSubTrigger>
                    <ContextMenuSubContent className="w-36">
                        <ContextMenuRadioGroup
                            value={levelFilter}
                            onValueChange={(val) => setLevelFilter(val as LevelFilter)}
                        >
                            {LEVEL_FILTERS.map((f) => (
                                <ContextMenuRadioItem key={f.value} value={f.value}>
                                    <span>{f.label}</span>
                                </ContextMenuRadioItem>
                            ))}
                        </ContextMenuRadioGroup>
                    </ContextMenuSubContent>
                </ContextMenuSub>
            </ContextMenuContent>
        </ContextMenu>
    );
}

function Toolbar({
    query,
    onQuery,
    levelFilter,
    onLevelFilter,
    autoScroll,
    onToggleAutoScroll,
    onClear,
    onCopy,
    hasLogs,
    hasVisible,
}: {
    query: string;
    onQuery: (s: string) => void;
    levelFilter: LevelFilter;
    onLevelFilter: (l: LevelFilter) => void;
    autoScroll: boolean;
    onToggleAutoScroll: () => void;
    onClear: () => void;
    onCopy: () => void;
    hasLogs: boolean;
    hasVisible: boolean;
}) {
    return (
        <div className="flex flex-wrap items-center gap-1 border-b border-border-subtle bg-elevated/40 px-2 py-1.5">
            <div className="relative min-w-[200px] flex-1">
                <Search
                    size={13}
                    className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                />
                <input
                    type="search"
                    value={query}
                    onChange={(e) => onQuery(e.target.value)}
                    placeholder="搜索关键字"
                    aria-label="搜索日志关键字"
                    className="h-7 w-full rounded-sm bg-transparent pl-7 pr-2 text-[12px] text-text outline-none transition-colors placeholder:text-text-tertiary hover:bg-inset/60 focus:bg-inset"
                />
            </div>
            <div
                role="radiogroup"
                aria-label="日志级别筛选"
                className="flex h-7 items-center gap-0.5 rounded-md bg-inset/60 p-0.5"
            >
                {LEVEL_FILTERS.map((f) => (
                    <button
                        key={f.value}
                        type="button"
                        role="radio"
                        aria-checked={levelFilter === f.value}
                        onClick={() => onLevelFilter(f.value)}
                        className={
                            'h-6 rounded-sm px-2 text-[11.5px] font-medium leading-6 transition-colors ' +
                            (levelFilter === f.value
                                ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                                : 'text-text-tertiary hover:text-text')
                        }
                    >
                        {f.label}
                    </button>
                ))}
            </div>
            <Button
                variant="ghost"
                size="sm"
                onClick={onToggleAutoScroll}
                title={autoScroll ? '已启用自动滚动' : '已暂停自动滚动'}
            >
                {autoScroll ? (
                    <ActionMotionIcon icon={Pause} size={13} motion={LIVE_MOTION} />
                ) : (
                    <ActionMotionIcon icon={Play} size={13} />
                )}
                <span className="ml-1 text-[11.5px]">{autoScroll ? '滚动中' : '已暂停'}</span>
            </Button>
            <Button variant="ghost" size="sm" onClick={onCopy} disabled={!hasVisible} title="复制当前可见日志">
                <ActionMotionIcon icon={Copy} size={13} />
                <span className="ml-1 text-[11.5px]">复制</span>
            </Button>
            <Button
                variant="ghost"
                size="sm"
                onClick={onClear}
                disabled={!hasLogs}
                title="清空面板（不删磁盘归档）"
            >
                <ActionMotionIcon icon={Brush} size={13} />
                <span className="ml-1 text-[11.5px]">清空</span>
            </Button>
        </div>
    );
}

function LogViewport({
    entries,
    emptyKind,
    emptyTitle,
    emptyBody,
    ariaLabel,
    autoScroll,
    selectedIds,
    onRowClick,
    onRowContextMenu,
}: {
    entries: LogEntry[];
    emptyKind: 'no-logs' | 'no-match' | 'has';
    emptyTitle: string;
    emptyBody: string;
    ariaLabel: string;
    autoScroll: boolean;
    selectedIds: Set<string>;
    onRowClick: (index: number, entry: LogEntry, e: React.MouseEvent) => void;
    onRowContextMenu: (index: number, entry: LogEntry, e: React.MouseEvent) => void;
}) {
    if (emptyKind === 'no-logs') {
        return (
            <EmptyState
                title={emptyTitle}
                body={emptyBody}
                icon={<ScrollText size={20} className="opacity-50" />}
            />
        );
    }
    if (emptyKind === 'no-match') {
        return (
            <EmptyState
                title="没有匹配的行"
                body="试试改下搜索关键字或切换级别筛选"
                icon={<Search size={20} className="opacity-50" />}
            />
        );
    }
    return (
        <VirtualLogList
            entries={entries}
            ariaLabel={ariaLabel}
            autoScroll={autoScroll}
            selectedIds={selectedIds}
            onRowClick={onRowClick}
            onRowContextMenu={onRowContextMenu}
        />
    );
}

function VirtualLogList({
    entries,
    ariaLabel,
    autoScroll,
    selectedIds,
    onRowClick,
    onRowContextMenu,
}: {
    entries: LogEntry[];
    ariaLabel: string;
    autoScroll: boolean;
    selectedIds: Set<string>;
    onRowClick: (index: number, entry: LogEntry, e: React.MouseEvent) => void;
    onRowContextMenu: (index: number, entry: LogEntry, e: React.MouseEvent) => void;
}) {
    const parentRef = useRef<HTMLDivElement | null>(null);
    const virtualizer = useVirtualizer({
        count: entries.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => LOG_ROW_HEIGHT_PX,
        overscan: 12,
    });

    useEffect(() => {
        if (!autoScroll || entries.length === 0) return;
        const id = requestAnimationFrame(() => {
            virtualizer.scrollToIndex(entries.length - 1, { align: 'end' });
        });
        return () => cancelAnimationFrame(id);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [entries.length, autoScroll]);

    const items = virtualizer.getVirtualItems();
    const totalSize = virtualizer.getTotalSize();

    return (
        <div
            ref={parentRef}
            role="log"
            aria-label={ariaLabel}
            aria-live="polite"
            className="min-h-0 flex-1 overflow-auto bg-inset font-mono text-[12px] leading-[18px]"
        >
            <div className="relative w-full py-1" style={{ height: totalSize }}>
                {items.map((virtualRow) => {
                    const entry = entries[virtualRow.index];
                    if (!entry) return null;
                    return (
                        <div
                            key={entry.id}
                            className="absolute left-0 top-0 w-full"
                            style={{
                                height: virtualRow.size,
                                transform: `translateY(${virtualRow.start}px)`,
                            }}
                        >
                            <LogLine
                                entry={entry}
                                isSelected={selectedIds.has(entry.id)}
                                onClick={(e) => onRowClick(virtualRow.index, entry, e)}
                                onContextMenu={(e) => onRowContextMenu(virtualRow.index, entry, e)}
                            />
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

function LogLine({
    entry,
    isSelected,
    onClick,
    onContextMenu,
}: {
    entry: LogEntry;
    isSelected: boolean;
    onClick: (e: React.MouseEvent) => void;
    onContextMenu: (e: React.MouseEvent) => void;
}) {
    return (
        <div
            onMouseDown={(e) => {
                e.preventDefault();
                window.getSelection()?.removeAllRanges();
            }}
            onClick={onClick}
            onContextMenu={onContextMenu}
            className={cn(
                'group flex h-[20px] cursor-pointer items-center gap-2 px-2 transition-colors select-none',
                isSelected
                    ? 'border-l-2 border-brand bg-brand/15 pl-[6px] font-medium text-text'
                    : 'hover:bg-elevated/70',
            )}
            style={{ userSelect: 'none', WebkitUserSelect: 'none' }}
        >
            <span className="h-[12px] w-[3px] shrink-0" style={{ background: levelBarColor(entry.level) }} />
            <span className="w-[58px] shrink-0 select-none whitespace-nowrap text-[11px] tabular-nums text-text-tertiary">
                {entry.timestamp}
            </span>
            <span
                className="w-[28px] shrink-0 select-none text-[10px] font-semibold uppercase tracking-wider"
                style={{ color: levelLabelColor(entry.level) }}
            >
                {LEVEL_LABEL[entry.level]}
            </span>
            <span className="min-w-0 flex-1 overflow-hidden truncate" style={{ color: lineTextColor(entry.level) }}>
                <HighlightedLogBody text={entry.text} level={entry.level} />
            </span>
        </div>
    );
}

function HighlightedLogBody({ text, level }: { text: string; level: LogEntry['level'] }) {
    if (!text) return '\u00A0';
    const pipeIdx = text.indexOf(' | ');
    if (pipeIdx >= 0) {
        const nick = text.slice(0, pipeIdx);
        const msg = text.slice(pipeIdx + 3);
        return (
            <>
                <span className="text-text-tertiary">{nick}</span>
                <span className="text-text-tertiary"> | </span>
                <span style={{ color: lineTextColor(level) }}>{msg}</span>
            </>
        );
    }
    return <>{text}</>;
}

function EmptyState({
    title,
    body,
    icon,
}: {
    title: string;
    body: string;
    icon?: React.ReactNode;
}) {
    return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 bg-inset/50 p-8 text-center text-text-tertiary">
            {icon}
            <p className="text-[13px] font-semibold text-text-secondary">{title}</p>
            <p className="text-[12px]">{body}</p>
        </div>
    );
}
