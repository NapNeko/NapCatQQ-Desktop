// Bot 日志环形缓冲。纯数据 + 纯函数。
//
// 历史快照（一次性）+ 增量 `bot_log_appended` / `snowluma_daemon_log` 双源。
// 上限 1000 行防止内存膨胀；超过时丢最早的。
//
// LogLevel 由本模块按行内容解析出来（trace / debug / info / success / warn /
// error / fatal / unknown），UI 只负责按 level 上颜色，不再做正则识别。

export type LogChannel = 'stdout' | 'stderr' | 'unknown';

export type LogLevel =
    | 'trace'
    | 'debug'
    | 'info'
    | 'success'
    | 'warn'
    | 'error'
    | 'fatal'
    | 'unknown';

export interface LogEntry {
    id: string;
    text: string;
    channel: LogChannel;
    level: LogLevel;
    timestamp: string;
}

const MAX_LINES = 1000;

let counter = 0;
function nextId(prefix = 'log'): string {
    counter += 1;
    return `${prefix}-${Date.now()}-${counter}`;
}

// 匹配 [info] / [INFO] / [Warn] 这种括号包裹的级别标签，对齐 legacy LogHighlighter
// 的 `\[(trace|debug|info|warn|warning|error|fatal|success)\]` 规则。
const BRACKET_LEVEL_PATTERN =
    /\[(trace|debug|info|warn|warning|error|fatal|success)\]/i;

// 匹配 NCD `2026-03-20 22:02:45 | INFO |` 这种竖线分隔级别。
const PIPE_LEVEL_PATTERN =
    /\|\s*(SUCCESS|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|TRACE)\s*\|/i;

// 匹配单词级别 ERROR / WARN 等独立出现在行首/词边界，作为 fallback。
// 严格要求两侧是非字母数字下划线，避免匹配到 `werror` / `traceback` 之类。
const STANDALONE_LEVEL_PATTERN =
    /(?:^|\W)(SUCCESS|FATAL|ERROR|WARNING|WARN|TRACE|DEBUG|INFO)(?:\W|$)/;

/// 从单行日志文本中提取级别。识别不到时返回 `'unknown'`。
/// 顺序：方括号 -> 竖线 -> 独立词，命中即停。
export function parseLogLevel(line: string): LogLevel {
    const bracket = line.match(BRACKET_LEVEL_PATTERN);
    if (bracket) {
        return normalizeLevel(bracket[1]);
    }
    const pipe = line.match(PIPE_LEVEL_PATTERN);
    if (pipe) {
        return normalizeLevel(pipe[1]);
    }
    const standalone = line.match(STANDALONE_LEVEL_PATTERN);
    if (standalone) {
        return normalizeLevel(standalone[1]);
    }
    return 'unknown';
}

function normalizeLevel(raw: string): LogLevel {
    switch (raw.toLowerCase()) {
        case 'trace':
            return 'trace';
        case 'debug':
            return 'debug';
        case 'info':
            return 'info';
        case 'success':
            return 'success';
        case 'warn':
        case 'warning':
            return 'warn';
        case 'error':
            return 'error';
        case 'fatal':
            return 'fatal';
        default:
            return 'unknown';
    }
}

// 桌面会话日志 preview：`26-06-10 19:55:40 | [INFO] | [ CORE ] bot_manager | ...`
const DESKTOP_PREVIEW_LEVEL = /\|\s*\[(EROR|WARN|INFO|DBUG|TRCE|CRIT)\]\s*\|/i;

/// 设置页 Desktop 日志行级别（对齐 legacy EROR/WARN/…）。
export function parseDesktopLogLevel(line: string): LogLevel {
    const m = line.match(DESKTOP_PREVIEW_LEVEL);
    if (!m) {
        return parseLogLevel(line);
    }
    switch (m[1].toUpperCase()) {
        case 'EROR':
            return 'error';
        case 'WARN':
            return 'warn';
        case 'INFO':
            return 'info';
        case 'DBUG':
            return 'debug';
        case 'TRCE':
            return 'trace';
        case 'CRIT':
            return 'fatal';
        default:
            return 'unknown';
    }
}

export function buildDesktopHistoryEntries(lines: string[]): LogEntry[] {
    const out: LogEntry[] = [];
    for (let idx = 0; idx < lines.length; idx++) {
        const raw = lines[idx];
        if (!raw || !raw.trim()) continue;
        const timeMatch = raw.match(/^(\d{2}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})/);
        out.push({
            id: `hist-${idx}-${counter++}`,
            text: raw,
            channel: 'unknown' as const,
            level: parseDesktopLogLevel(raw),
            timestamp: timeMatch?.[1] ?? '',
        });
    }
    return out;
}

export function serializeDesktopLogs(logs: LogEntry[]): string {
    return logs.map((l) => l.text).join('\n');
}

export function buildHistoryEntries(
    lines: string[],
    now = new Date().toLocaleTimeString(),
): LogEntry[] {
    const out: LogEntry[] = [];
    for (let idx = 0; idx < lines.length; idx++) {
        const raw = lines[idx];
        if (!raw || !raw.trim()) continue;
        out.push({
            id: `hist-${idx}-${counter++}`,
            text: raw,
            channel: 'unknown' as const,
            level: parseLogLevel(raw),
            timestamp: now,
        });
    }
    return out;
}

export function appendLine(
    prev: LogEntry[],
    line: string,
    channel: LogChannel,
): LogEntry[] {
    if (!line || !line.trim()) return prev;
    const entry: LogEntry = {
        id: nextId(),
        text: line,
        channel,
        level: parseLogLevel(line),
        timestamp: new Date().toLocaleTimeString(),
    };
    const next = [...prev, entry];
    if (next.length > MAX_LINES) {
        return next.slice(next.length - MAX_LINES);
    }
    return next;
}

/// 把 `event.channel` 字符串收敛到 stdout / stderr / unknown 三档。
export function normalizeChannel(raw: string | null | undefined): LogChannel {
    if (raw === 'stdout' || raw === 'stderr') return raw;
    return 'unknown';
}

/// SnowLuma daemon log 行：以 `[stderr]` 前缀分流到 stderr，否则 stdout。
/// SL backend 在 daemon spawn 时给 stderr 行加了 `[stderr]` 前缀，
/// stdout 行不加，于是这里靠前缀分流。
export function snowlumaLineChannel(line: string): LogChannel {
    return line.startsWith('[stderr]') ? 'stderr' : 'stdout';
}

export type ChannelFilter = 'all' | LogChannel;
export type LevelFilter = 'all' | LogLevel;

export function filterLogs(
    logs: LogEntry[],
    query: string,
    channelFilter: ChannelFilter,
    levelFilter: LevelFilter = 'all',
): LogEntry[] {
    const q = query.toLowerCase();
    return logs.filter((log) => {
        const matchesSearch = !q || log.text.toLowerCase().includes(q);
        const matchesChannel = channelFilter === 'all' || log.channel === channelFilter;
        const matchesLevel = levelFilter === 'all' || log.level === levelFilter;
        return matchesSearch && matchesChannel && matchesLevel;
    });
}

export function serializeLogs(logs: LogEntry[]): string {
    return logs
        .map(
            (l) =>
                `[${l.timestamp}] [${l.level.toUpperCase()}/${l.channel.toUpperCase()}] ${l.text}`,
        )
        .join('\n');
}

/// 按级别给一个用于 BotCard / BotLogPage 的色调标签，
/// 调用方可据此挑 Tailwind class / Fluent Badge color。
export function logLevelTone(level: LogLevel): 'danger' | 'warning' | 'success' | 'info' | 'neutral' {
    switch (level) {
        case 'fatal':
        case 'error':
            return 'danger';
        case 'warn':
            return 'warning';
        case 'success':
            return 'success';
        case 'info':
            return 'info';
        case 'trace':
        case 'debug':
        case 'unknown':
        default:
            return 'neutral';
    }
}
