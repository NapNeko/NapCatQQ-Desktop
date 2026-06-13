// 推倒重写版 BotLogPage：
//
// 视觉：和项目暖白基调对齐（不用 GitHub 黑），日志面板用 surface-inset。
// 布局：
//   顶部 一行 返回按钮 + 标题 + 行数 chip
//   工具栏 一行 搜索框 + 级别 chip + 自动滚动 + 复制 + 清空
//   主体  flex-1 overflow-auto，每行高 22px 单行，左侧 2px 彩色条表 level
// 行结构 grid 三列：时间戳 / level 标签 / 文本（文本内 break-all）
// 历史与增量数据来自 useBotLogStream，不直接调 service。

import { forwardRef, useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import {
    ArrowLeft,
    Brush,
    Copy,
    Search,
    ScrollText,
    Pause,
    Play,
} from 'lucide-react';
import { Badge, Button } from '../../../shared/ui';
import { ActionMotionIcon, LIVE_MOTION } from '../../../shared/ui/motion';
import {
    filterLogs,
    serializeLogs,
    type ChannelFilter,
    type LevelFilter,
    type LogEntry,
} from '../../../core/domain/events/log-buffer';
import {
    LOG_LEVEL_SHORT,
    levelBarColor,
    levelLabelColor,
    lineTextColor,
} from '../../../shared/log/log-level-display';
import { useBotLogStream } from '../../../hooks/bot/useBotLogStream';

interface BotLogPageNextProps {
    botId: string;
    onBack: () => void;
}

const LEVEL_LABEL = LOG_LEVEL_SHORT;

const LEVEL_FILTERS: { value: LevelFilter; label: string }[] = [
    { value: 'all', label: '全部' },
    { value: 'info', label: '信息' },
    { value: 'warn', label: '警告' },
    { value: 'error', label: '错误' },
    { value: 'debug', label: '调试' },
];

export function BotLogPageNext({ botId, onBack }: BotLogPageNextProps) {
    const { logs, clear } = useBotLogStream(botId);
    const [query, setQuery] = useState('');
    const [channelFilter] = useState<ChannelFilter>('all');
    const [levelFilter, setLevelFilter] = useState<LevelFilter>('all');
    const [autoScroll, setAutoScroll] = useState(true);
    const containerRef = useRef<HTMLDivElement>(null);

    const filtered = useMemo(
        () => filterLogs(logs, query, channelFilter, levelFilter),
        [logs, query, channelFilter, levelFilter],
    );

    useEffect(() => {
        if (!autoScroll) return;
        const el = containerRef.current;
        if (el) el.scrollTop = el.scrollHeight;
    }, [filtered.length, autoScroll]);

    const onCopy = async () => {
        if (filtered.length === 0) return;
        try {
            await navigator.clipboard.writeText(serializeLogs(filtered));
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('复制日志失败:', err);
        }
    };

    const emptyKind: 'no-logs' | 'no-match' | 'has' =
        logs.length === 0 ? 'no-logs' : filtered.length === 0 ? 'no-match' : 'has';

    return (
        <div className="flex h-full min-h-0 flex-col gap-3">
            <Header
                botId={botId}
                onBack={onBack}
                total={logs.length}
                shown={filtered.length}
            />
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md ring-1 ring-border-subtle">
                <Toolbar
                    query={query}
                    onQuery={setQuery}
                    levelFilter={levelFilter}
                    onLevelFilter={setLevelFilter}
                    autoScroll={autoScroll}
                    onToggleAutoScroll={() => setAutoScroll((p) => !p)}
                    onClear={clear}
                    onCopy={onCopy}
                    hasLogs={logs.length > 0}
                    hasVisible={filtered.length > 0}
                />
                <LogViewport
                    ref={containerRef}
                    entries={filtered}
                    emptyKind={emptyKind}
                />
            </div>
        </div>
    );
}

export default BotLogPageNext;


// ============================================================================
// 子组件
// ============================================================================

function Header({
    botId,
    onBack,
    total,
    shown,
}: {
    botId: string;
    onBack: () => void;
    total: number;
    shown: number;
}) {
    return (
        <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
                <ActionMotionIcon icon={ArrowLeft} size={16} />
            </Button>
            <h2 className="text-[15px] font-semibold leading-none text-text">
                实例 {botId} 运行日志
            </h2>
            <Badge tone="neutral" appearance="soft">
                {total} 行
            </Badge>
            {shown !== total && (
                <Badge tone="info" appearance="soft">
                    筛后 {shown}
                </Badge>
            )}
        </div>
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
            {/* 搜索框：弱化视觉，hover/focus 才显示 inset 底色 */}
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
            {/* 级别筛选：单选胶囊条,语义上是 radiogroup(单选) */}
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
            {/* 自动滚动 */}
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
                <span className="ml-1 text-[11.5px]">
                    {autoScroll ? '滚动中' : '已暂停'}
                </span>
            </Button>
            {/* 复制 */}
            <Button
                variant="ghost"
                size="sm"
                onClick={onCopy}
                disabled={!hasVisible}
                title="复制当前可见日志"
            >
                <ActionMotionIcon icon={Copy} size={13} />
                <span className="ml-1 text-[11.5px]">复制</span>
            </Button>
            {/* 清空 */}
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


const LOG_ROW_HEIGHT_PX = 20;

const LogViewport = forwardRef<
    HTMLDivElement,
    { entries: LogEntry[]; emptyKind: 'no-logs' | 'no-match' | 'has' }
>(function LogViewport({ entries, emptyKind }, ref) {
    if (emptyKind === 'no-logs') {
        return (
            <EmptyState
                title="暂无日志"
                body="实例可能尚未启动，或当前还没触发任何输出"
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
            ref={ref}
            entries={entries}
            rowHeight={LOG_ROW_HEIGHT_PX}
        />
    );
});

function VirtualLogList({
    entries,
    rowHeight,
    ref: forwardedRef,
}: {
    entries: LogEntry[];
    rowHeight: number;
    ref: React.ForwardedRef<HTMLDivElement>;
}) {
    const parentRef = useRef<HTMLDivElement | null>(null);

    const setRefs = (el: HTMLDivElement | null) => {
        parentRef.current = el;
        if (typeof forwardedRef === 'function') {
            forwardedRef(el);
        } else if (forwardedRef) {
            forwardedRef.current = el;
        }
    };

    const virtualizer = useVirtualizer({
        count: entries.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => rowHeight,
        overscan: 12,
    });

    const items = virtualizer.getVirtualItems();
    const totalSize = virtualizer.getTotalSize();

    return (
        <div
            ref={setRefs}
            role="log"
            aria-label="实例运行日志"
            aria-live="polite"
            className="min-h-0 flex-1 overflow-auto bg-inset font-mono text-[12px] leading-[18px]"
        >
            <div
                className="relative w-full py-1"
                style={{ height: totalSize }}
            >
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
                            <LogLine entry={entry} />
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

function LogLine({ entry }: { entry: LogEntry }) {
    return (
        <div className="group flex h-[20px] items-center gap-2 px-2 hover:bg-elevated">
            <span
                className="h-[12px] w-[3px] shrink-0"
                style={{ background: levelBarColor(entry.level) }}
            />
            <span className="w-[58px] shrink-0 select-none text-[11px] tabular-nums text-text-tertiary">
                {entry.timestamp}
            </span>
            <span
                className="w-[28px] shrink-0 select-none text-[10px] font-semibold uppercase tracking-wider"
                style={{ color: levelLabelColor(entry.level) }}
            >
                {LEVEL_LABEL[entry.level]}
            </span>
            <span
                className="min-w-0 flex-1 overflow-hidden truncate"
                style={{ color: lineTextColor(entry.level) }}
                title={entry.text}
            >
                {entry.text || '\u00A0'}
            </span>
        </div>
    );
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
