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
import {
    filterLogs,
    serializeLogs,
    type ChannelFilter,
    type LevelFilter,
    type LogEntry,
    type LogLevel,
} from '../../../core/domain/events/log-buffer';
import { useBotLogStream } from '../../../hooks/bot/useBotLogStream';

interface BotLogPageNextProps {
    botId: string;
    onBack: () => void;
}

const LEVEL_LABEL: Record<LogLevel, string> = {
    trace: 'TRC',
    debug: 'DBG',
    info: 'INF',
    success: 'OK',
    warn: 'WRN',
    error: 'ERR',
    fatal: 'FTL',
    unknown: '·',
};

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
        <div className="flex h-full min-h-0 flex-col gap-2">
            <Header
                botId={botId}
                onBack={onBack}
                total={logs.length}
                shown={filtered.length}
            />
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
            <LogViewport ref={containerRef} entries={filtered} emptyKind={emptyKind} />
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
                <ArrowLeft size={16} />
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
        <div className="flex flex-wrap items-center gap-1 border-b border-border-subtle pb-2">
            {/* 搜索框：弱化视觉，无背景无边框，hover/focus 才浮起 */}
            <div className="relative min-w-[200px] flex-1">
                <Search
                    size={13}
                    className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                />
                <input
                    value={query}
                    onChange={(e) => onQuery(e.target.value)}
                    placeholder="搜索关键字"
                    className="h-7 w-full bg-transparent pl-7 pr-2 text-[12px] text-text outline-none transition-colors placeholder:text-text-tertiary hover:bg-inset/60 focus:bg-inset"
                />
            </div>
            {/* 级别筛选：单一胶囊条，inset 底色，active 项高亮 */}
            <div className="flex h-7 items-center gap-0.5 bg-inset/60 p-0.5">
                {LEVEL_FILTERS.map((f) => (
                    <button
                        key={f.value}
                        type="button"
                        onClick={() => onLevelFilter(f.value)}
                        className={
                            'h-6 px-2 text-[11.5px] font-medium leading-6 transition-colors ' +
                            (levelFilter === f.value
                                ? 'bg-surface text-text'
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
                {autoScroll ? <Pause size={13} /> : <Play size={13} />}
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
                <Copy size={13} />
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
                <Brush size={13} />
                <span className="ml-1 text-[11.5px]">清空</span>
            </Button>
        </div>
    );
}


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
        <div
            ref={ref}
            className="min-h-0 flex-1 overflow-auto bg-inset font-mono text-[12px] leading-[18px]"
            style={{ fontFamily: 'var(--font-mono)' }}
        >
            <div className="py-1">
                {entries.map((e) => (
                    <LogLine key={e.id} entry={e} />
                ))}
            </div>
        </div>
    );
});

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

// 暖色调下的 level 颜色映射，与 design tokens 的 state-* 对齐但稍微提亮，
// 让小字号下也能在浅米色背景上区分。
function levelBarColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--state-warning)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
            return 'var(--state-info)';
        case 'debug':
        case 'trace':
            return 'var(--neutral-300, #d1c4b6)';
        default:
            return 'transparent';
    }
}

function levelLabelColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--state-warning)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
            return 'var(--state-info)';
        case 'debug':
        case 'trace':
            return 'var(--text-tertiary)';
        default:
            return 'var(--text-disabled)';
    }
}

function lineTextColor(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'var(--state-danger)';
        case 'warn':
            return 'var(--text-primary)';
        case 'success':
            return 'var(--state-success)';
        case 'info':
        case 'debug':
        case 'trace':
            return 'var(--text-secondary)';
        default:
            return 'var(--text-secondary)';
    }
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
