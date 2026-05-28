// 推倒重写版 BotLogPage：
//
// 视觉：暗色控制台风，顶部一条工具栏 + 中部彩色行 + 底部统计/状态。
// 交互：搜索筛选、级别筛选、清空、复制、自动滚动开关。
// 行为：复用 useBotLogStream（hook 层），不直接调 service。
//
// 跟旧版的差异：
//   - 不再用 Fluent Button/Badge/Input；改用 shared/ui + tailwind。
//   - channel 的 stdout/stderr 标签改为 LEVEL（info/warn/error...）+ 行首时间戳。
//   - 增加级别筛选 chip 行。
//   - 空状态文案沿用旧版语义但更简洁。

import { forwardRef, useEffect, useMemo, useRef, useState } from 'react';
import {
    ArrowLeft,
    Brush,
    Copy,
    Search,
    ScrollText,
    Pause,
    PlayCircle,
} from 'lucide-react';
import { Badge, Button } from '../../../shared/ui';
import {
    filterLogs,
    serializeLogs,
    logLevelTone,
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
    trace: 'TRACE',
    debug: 'DEBUG',
    info: 'INFO',
    success: 'OK',
    warn: 'WARN',
    error: 'ERR',
    fatal: 'FATAL',
    unknown: '—',
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
    const [channelFilter, _setChannelFilter] = useState<ChannelFilter>('all');
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
            // 剪贴板失败不弹错（可能用户没授权），打个 console 日志即可。
            // eslint-disable-next-line no-console
            console.warn('复制日志失败:', err);
        }
    };

    return (
        <div className="flex h-full flex-col gap-3">
            <Header botId={botId} onBack={onBack} total={logs.length} shown={filtered.length} />
            <Toolbar
                query={query}
                onQuery={setQuery}
                levelFilter={levelFilter}
                onLevelFilter={setLevelFilter}
                autoScroll={autoScroll}
                onToggleAutoScroll={() => setAutoScroll((prev) => !prev)}
                onClear={clear}
                onCopy={onCopy}
                hasLogs={logs.length > 0}
                hasVisible={filtered.length > 0}
            />
            <LogViewport
                ref={containerRef}
                entries={filtered}
                emptyKind={
                    logs.length === 0 ? 'no-logs' : filtered.length === 0 ? 'no-match' : 'has'
                }
            />
        </div>
    );
}

export default BotLogPageNext;


// ============================================================================
// 子组件 Header / Toolbar / LogViewport / LogLine
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
        <div className="flex items-start gap-3">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
                <ArrowLeft size={16} />
            </Button>
            <div className="flex flex-1 flex-col gap-1">
                <div className="flex items-baseline gap-2">
                    <h2 className="text-base font-semibold text-text">实例 {botId} 运行日志</h2>
                    <Badge tone="neutral" appearance="soft">
                        共 {total} 行
                    </Badge>
                    {shown !== total && (
                        <Badge tone="info" appearance="soft">
                            筛后 {shown} 行
                        </Badge>
                    )}
                </div>
                <p className="text-[12px] text-text-tertiary">
                    实时订阅子进程输出。NapCat 重启时旧日志自动清空；SnowLuma 多 Bot 共享 daemon 输出
                </p>
            </div>
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
        <div className="flex flex-wrap items-center gap-2 rounded-md bg-elevated/60 p-2 ring-1 ring-border-subtle">
            {/* 搜索框 */}
            <div className="relative min-w-[200px] flex-1">
                <Search
                    size={14}
                    className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                />
                <input
                    value={query}
                    onChange={(e) => onQuery(e.target.value)}
                    placeholder="搜索关键字 (Ctrl+F 风格)"
                    className="h-8 w-full rounded-md bg-inset pl-7 pr-2 text-[13px] text-text outline-none ring-1 ring-border-subtle focus:ring-brand"
                />
            </div>
            {/* 级别筛选 */}
            <div className="flex items-center gap-1">
                {LEVEL_FILTERS.map((f) => (
                    <button
                        key={f.value}
                        type="button"
                        onClick={() => onLevelFilter(f.value)}
                        className={
                            'rounded-md px-2 py-1 text-[12px] transition-colors ' +
                            (levelFilter === f.value
                                ? 'bg-brand text-white'
                                : 'bg-inset text-text-secondary hover:bg-inset-hover')
                        }
                    >
                        {f.label}
                    </button>
                ))}
            </div>
            {/* 自动滚动开关 */}
            <Button
                variant={autoScroll ? 'secondary' : 'ghost'}
                size="sm"
                onClick={onToggleAutoScroll}
                title={autoScroll ? '已启用自动滚动到底' : '已暂停自动滚动'}
            >
                {autoScroll ? <Pause size={14} /> : <PlayCircle size={14} />}
                <span className="ml-1">{autoScroll ? '滚动中' : '已暂停'}</span>
            </Button>
            {/* 复制 */}
            <Button variant="ghost" size="sm" onClick={onCopy} disabled={!hasVisible}>
                <Copy size={14} />
                <span className="ml-1">复制</span>
            </Button>
            {/* 清空 */}
            <Button variant="ghost" size="sm" onClick={onClear} disabled={!hasLogs}>
                <Brush size={14} />
                <span className="ml-1">清空</span>
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
                icon={<ScrollText size={18} className="opacity-60" />}
            />
        );
    }
    if (emptyKind === 'no-match') {
        return (
            <EmptyState
                title="没有匹配的行"
                body="试试修改搜索关键字或切换级别筛选"
                icon={<Search size={18} className="opacity-60" />}
            />
        );
    }
    return (
        <div
            ref={ref}
            className="flex-1 overflow-auto rounded-md bg-[#0d1117] font-mono text-[12.5px] leading-[1.55] ring-1 ring-border-subtle"
            style={{ fontFamily: '"JetBrains Mono", "Fira Code", Consolas, monospace' }}
        >
            {entries.map((e) => (
                <LogLine key={e.id} entry={e} />
            ))}
        </div>
    );
});

function LogLine({ entry }: { entry: LogEntry }) {
    const tone = logLevelTone(entry.level);
    const levelClass = logLevelToClass(entry.level);
    return (
        <div className="grid grid-cols-[68px_44px_1fr] items-start gap-2 px-3 py-[3px] hover:bg-white/[0.03]">
            <span className="select-none text-[#7f8c98]">{entry.timestamp}</span>
            <span className={'select-none text-[10.5px] uppercase tabular-nums ' + levelClass}>
                {LEVEL_LABEL[entry.level]}
            </span>
            <span className={'whitespace-pre-wrap break-all ' + lineTextClass(tone)}>
                {entry.text}
            </span>
        </div>
    );
}

function logLevelToClass(level: LogLevel): string {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'text-[#ff7b7b]';
        case 'warn':
            return 'text-[#f5c971]';
        case 'success':
            return 'text-[#9ece6a]';
        case 'info':
            return 'text-[#7aa2f7]';
        case 'debug':
        case 'trace':
            return 'text-[#9aa5ce]';
        default:
            return 'text-[#7f8c98]';
    }
}

function lineTextClass(tone: ReturnType<typeof logLevelTone>): string {
    switch (tone) {
        case 'danger':
            return 'text-[#fca5a5]';
        case 'warning':
            return 'text-[#fcd34d]';
        case 'success':
            return 'text-[#bbf7d0]';
        case 'info':
            return 'text-[#bfdbfe]';
        case 'neutral':
        default:
            return 'text-[#d1d5db]';
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
        <div className="flex flex-1 flex-col items-center justify-center gap-2 rounded-md bg-elevated/40 p-6 text-center text-text-tertiary ring-1 ring-border-subtle">
            {icon}
            <p className="text-[13px] font-semibold text-text-secondary">{title}</p>
            <p className="text-[12px]">{body}</p>
        </div>
    );
}
