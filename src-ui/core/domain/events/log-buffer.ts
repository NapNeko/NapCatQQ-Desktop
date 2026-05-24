// Bot 日志环形缓冲。纯数据 + 纯函数。
//
// 历史快照（一次性）+ 增量 `bot_log_appended` / `snowluma_daemon_log` 双源。
// 上限 1000 行防止内存膨胀；超过时丢最早的。

export type LogChannel = 'stdout' | 'stderr' | 'unknown';

export interface LogEntry {
    id: string;
    text: string;
    channel: LogChannel;
    timestamp: string;
}

const MAX_LINES = 1000;

let counter = 0;
function nextId(prefix = 'log'): string {
    counter += 1;
    return `${prefix}-${Date.now()}-${counter}`;
}

export function buildHistoryEntries(
    lines: string[],
    now = new Date().toLocaleTimeString(),
): LogEntry[] {
    return lines.map((line, idx) => ({
        id: `hist-${idx}-${counter++}`,
        text: line,
        channel: 'unknown' as const,
        timestamp: now,
    }));
}

export function appendLine(
    prev: LogEntry[],
    line: string,
    channel: LogChannel,
): LogEntry[] {
    const entry: LogEntry = {
        id: nextId(),
        text: line,
        channel,
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
export function snowlumaLineChannel(line: string): LogChannel {
    return line.startsWith('[stderr]') ? 'stderr' : 'stdout';
}

export function filterLogs(
    logs: LogEntry[],
    query: string,
    channelFilter: 'all' | LogChannel,
): LogEntry[] {
    const q = query.toLowerCase();
    return logs.filter((log) => {
        const matchesSearch = !q || log.text.toLowerCase().includes(q);
        const matchesChannel = channelFilter === 'all' || log.channel === channelFilter;
        return matchesSearch && matchesChannel;
    });
}

export function serializeLogs(logs: LogEntry[]): string {
    return logs
        .map((l) => `[${l.timestamp}] [${l.channel.toUpperCase()}] ${l.text}`)
        .join('\n');
}
