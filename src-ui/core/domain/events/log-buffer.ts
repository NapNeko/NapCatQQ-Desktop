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
    /** Desktop preview：来源 + 模块，如 `[ CORE ] bot_manager` */
    context?: string;
    /** Desktop preview：原始等级标签，如 `[INFO]` */
    levelTag?: string;
    /** Desktop：完整 preview 行，复制用 */
    rawLine?: string;
}

const MAX_LINES = 1000;

let counter = 0;
function nextId(prefix = 'log'): string {
    counter += 1;
    return `${prefix}-${Date.now()}-${counter}`;
}

// Karin chalk：`[Karin][20:16:22.716][MARK]`。MARK 是灰标，对到 trace。
const BRACKET_LEVEL_PATTERN =
    /\[(trace|debug|info|warn|warning|error|fatal|success|mark)\]/i;

// 匹配 NCD `2026-03-20 22:02:45 | INFO |` 这种竖线分隔级别。
const PIPE_LEVEL_PATTERN =
    /\|\s*(SUCCESS|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|TRACE)\s*\|/i;

// 匹配单词级别 ERROR / WARN 等独立出现在行首/词边界，作为 fallback。
// 严格要求两侧是非字母数字下划线，避免匹配到 `werror` / `traceback` 之类。
const STANDALONE_LEVEL_PATTERN =
    /(?:^|\W)(SUCCESS|FATAL|ERROR|WARNING|WARN|TRACE|DEBUG|INFO)(?:\W|$)/;

/// 剥 CSI / OSC / 残余 ESC。Karin chalk、NC 颜色码都走这里再解析等级。
export function stripAnsiEscapes(input: string): string {
    let out = '';
    let i = 0;
    const n = input.length;
    while (i < n) {
        const c = input.charCodeAt(i);
        if (c === 0x1b) {
            const next = i + 1 < n ? input.charCodeAt(i + 1) : 0;
            if (next === 0x5b) {
                i = consumeCsi(input, i + 2);
                continue;
            }
            if (next === 0x5d || next === 0x50 || next === 0x58 || next === 0x5e || next === 0x5f) {
                i = consumeStringSeq(input, i + 2);
                continue;
            }
            i += next ? 2 : 1;
            continue;
        }
        if (c === 0x9b) {
            i = consumeCsi(input, i + 1);
            continue;
        }
        if (c === 0x7f || (c < 0x20 && c !== 0x09 && c !== 0x0a && c !== 0x0d)) {
            i += 1;
            continue;
        }
        out += input[i];
        i += 1;
    }
    return out;
}

function consumeCsi(input: string, start: number): number {
    let i = start;
    const n = input.length;
    while (i < n) {
        const b = input.charCodeAt(i);
        if (b >= 0x30 && b <= 0x3f) {
            i += 1;
            continue;
        }
        break;
    }
    while (i < n) {
        const b = input.charCodeAt(i);
        if (b >= 0x20 && b <= 0x2f) {
            i += 1;
            continue;
        }
        break;
    }
    if (i < n) {
        const b = input.charCodeAt(i);
        if (b >= 0x40 && b <= 0x7e) i += 1;
    }
    return i;
}

function consumeStringSeq(input: string, start: number): number {
    let i = start;
    const n = input.length;
    while (i < n) {
        const b = input.charCodeAt(i);
        if (b === 0x07) return i + 1;
        if (b === 0x1b && i + 1 < n && input.charCodeAt(i + 1) === 0x5c) return i + 2;
        i += 1;
    }
    return i;
}

/// 从单行日志文本中提取级别。识别不到时返回 `'unknown'`。
/// 顺序：方括号 -> 竖线 -> 独立词，命中即停。
export function parseLogLevel(line: string): LogLevel {
    const cleaned = stripAnsiEscapes(line);
    const bracket = cleaned.match(BRACKET_LEVEL_PATTERN);
    if (bracket) {
        return normalizeLevel(bracket[1]);
    }
    const pipe = cleaned.match(PIPE_LEVEL_PATTERN);
    if (pipe) {
        return normalizeLevel(pipe[1]);
    }
    const standalone = cleaned.match(STANDALONE_LEVEL_PATTERN);
    if (standalone) {
        return normalizeLevel(standalone[1]);
    }
    return 'unknown';
}

function normalizeLevel(raw: string): LogLevel {
    switch (raw.toLowerCase()) {
        case 'mark':
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


// NapCat 控制台: `07-11 17:06:19 [info] nick | msg`（sanitize 后）
// 捕获组: 月日 / 时分秒 / 正文
const NAPCAT_TS_PREFIX =
    /^(\d{1,2}-\d{1,2})\s+(\d{1,2}:\d{2}:\d{2})\s+(.*)$/;

// SnowLuma daemon: 22:21:24 INFO               [App] ...（无月日）
const SNOWLUMA_TS_PREFIX =
    /^(\d{1,2}:\d{2}:\d{2})\s+(INFO|WARN|WARNING|ERROR|DEBUG|TRACE|OK|FATAL)?\s*(.*)$/i;

// 行首等级标签；UI 已有 INF/ERR 列，正文里再显示会重复
const LEADING_LEVEL_TAG =
    /^\[(?:trace|debug|info|warn|warning|error|fatal|success|mark)\]\s*/i;

// `[Karin][20:16:22.716][INFO] body` — 时分秒给时间列，LEVEL 给色条，信封不进正文
const FRAMEWORK_TS_LEVEL =
    /^\[([^\]]+)\]\[(\d{1,2}:\d{2}:\d{2})(?:\.\d+)?\]\[([A-Za-z]+)\]\s*(.*)$/;

/** 列表时间列只放 HH:mm:ss；整段 `MM-DD HH:mm:ss` 塞 20px 行会折成只剩日期 */
function normalizeClock(ts: string): string {
    const m = ts.match(/(\d{1,2}):(\d{2}):(\d{2})/);
    if (!m) return ts;
    return `${m[1].padStart(2, '0')}:${m[2]}:${m[3]}`;
}

/** 去掉正文行首 `[info]` 等；level 仍由 parseLogLevel(整行) 负责 */
export function stripLeadingLevelTag(body: string): string {
    return body.replace(LEADING_LEVEL_TAG, '').trimStart();
}

/** 拆 NC 时间前缀；无匹配时 ts 用 fallback，body 为整行（仍可能带 [level]） */
export function splitLogTimestamp(
    line: string,
    fallbackTs: string,
): { timestamp: string; body: string } {
    const cleaned = stripAnsiEscapes(line);
    const fw = cleaned.match(FRAMEWORK_TS_LEVEL);
    if (fw) {
        return {
            timestamp: normalizeClock(fw[2]),
            body: fw[4],
        };
    }
    const m = cleaned.match(NAPCAT_TS_PREFIX);
    if (m) {
        return {
            timestamp: normalizeClock(m[2]),
            body: stripLeadingLevelTag(m[3]),
        };
    }
    const sl = cleaned.match(SNOWLUMA_TS_PREFIX);
    if (sl) {
        const body = (sl[3] ?? '').trimStart();
        return {
            timestamp: normalizeClock(sl[1]),
            body: stripLeadingLevelTag(body || cleaned),
        };
    }
    return {
        timestamp: normalizeClock(fallbackTs),
        body: stripLeadingLevelTag(cleaned),
    };
}

// 桌面会话 preview 四段：`时间 | [INFO] | [ CORE ] bot_manager | 说明`
const DESKTOP_PREVIEW_LEVEL = /\|\s*\[(EROR|WARN|INFO|DBUG|TRCE|CRIT)\]\s*\|/i;

const DESKTOP_PREVIEW_FOUR_PART =
    /^(\d{2}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})\s*\|\s*(\[[^\]]+\])\s*\|\s*(.+?)\s*\|\s*(.+)$/;

export interface ParsedDesktopLogLine {
    timestamp: string;
    levelTag: string;
    level: LogLevel;
    context: string;
    message: string;
    raw: string;
}

function desktopLegacyLevelToLogLevel(tag: string): LogLevel {
    const inner = tag.replace(/^\[|\]$/g, '').trim().toUpperCase();
    switch (inner) {
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

/// 解析设置页 tail 返回的 preview 行；非四段时退化为整行作 message。
export function parseDesktopLogLine(line: string): ParsedDesktopLogLine {
    const raw = line.replace(/\r?\n+$/, '').trimEnd();
    const m = raw.match(DESKTOP_PREVIEW_FOUR_PART);
    if (m) {
        const levelTag = m[2].trim();
        return {
            timestamp: m[1],
            levelTag,
            level: desktopLegacyLevelToLogLevel(levelTag),
            context: m[3].trim(),
            message: m[4].trim(),
            raw,
        };
    }
    const timeMatch = raw.match(/^(\d{2}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})/);
    return {
        timestamp: timeMatch?.[1] ?? '',
        levelTag: '',
        level: parseDesktopLogLevel(raw),
        context: '',
        message: raw,
        raw,
    };
}

/// 设置页 Desktop 日志行级别（对齐 legacy EROR/WARN/…）。
export function parseDesktopLogLevel(line: string): LogLevel {
    const m = line.match(DESKTOP_PREVIEW_LEVEL);
    if (!m) {
        return parseLogLevel(line);
    }
    return desktopLegacyLevelToLogLevel(`[${m[1]}]`);
}

function fnv1a32(str: string): number {
    let hash = 2166136261;
    for (let i = 0; i < str.length; i++) {
        hash ^= str.charCodeAt(i);
        hash = Math.imul(hash, 16777619);
    }
    return hash >>> 0;
}

export function buildDesktopHistoryEntries(lines: string[]): LogEntry[] {
    const out: LogEntry[] = [];
    const seen = new Map<string, number>();
    for (let idx = 0; idx < lines.length; idx++) {
        const raw = lines[idx];
        if (!raw || !raw.trim()) continue;
        const parsed = parseDesktopLogLine(raw);
        const baseKey = `dlog-${parsed.timestamp.replace(/\s+/g, '_') || idx}-${fnv1a32(raw).toString(36)}`;
        const count = seen.get(baseKey) ?? 0;
        seen.set(baseKey, count + 1);
        const id = count === 0 ? baseKey : `${baseKey}-${count}`;
        out.push({
            id,
            text: parsed.message,
            channel: 'unknown' as const,
            level: parsed.level,
            timestamp: parsed.timestamp,
            context: parsed.context || undefined,
            levelTag: parsed.levelTag || undefined,
            rawLine: parsed.raw,
        });
    }
    return out;
}

export function serializeDesktopLogs(logs: LogEntry[]): string {
    return logs.map((l) => l.rawLine ?? l.text).join('\n');
}

export function buildHistoryEntries(
    lines: string[],
    now = new Date().toLocaleTimeString(),
): LogEntry[] {
    const out: LogEntry[] = [];
    for (let idx = 0; idx < lines.length; idx++) {
        const raw = lines[idx];
        if (!raw || !raw.trim()) continue;
        const { timestamp, body } = splitLogTimestamp(raw, now);
        out.push({
            id: `hist-${idx}-${counter++}`,
            text: body,
            channel: 'unknown' as const,
            level: parseLogLevel(raw),
            timestamp,
        });
    }
    return out;
}

export function appendLine(
    logs: LogEntry[],
    line: string,
    channel: LogChannel = 'stdout',
    now = new Date().toLocaleTimeString(),
): LogEntry[] {
    const cleaned = stripAnsiEscapes(line);
    if (!cleaned || !cleaned.trim()) return logs;
    const { timestamp, body } = splitLogTimestamp(cleaned, now);
    const entry: LogEntry = {
        id: nextId(),
        text: body,
        channel,
        level: parseLogLevel(cleaned),
        timestamp,
    };
    const next =
        logs.length >= MAX_LINES ? logs.slice(logs.length - MAX_LINES + 1) : logs.slice();
    next.push(entry);
    return next;
}

/// 会话里已经进过缓冲的脏行（未剥 ANSI / 还带着 Karin 信封）再洗一遍。
export function canonicalizeLogEntry(entry: LogEntry): LogEntry {
    const cleaned = stripAnsiEscapes(entry.text);
    if (cleaned === entry.text && !FRAMEWORK_TS_LEVEL.test(cleaned)) {
        return entry;
    }
    const { timestamp, body } = splitLogTimestamp(cleaned, entry.timestamp);
    const level = parseLogLevel(cleaned);
    if (body === entry.text && level === entry.level && timestamp === entry.timestamp) {
        return entry;
    }
    return { ...entry, text: body, level, timestamp };
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
        const haystack = [log.text, log.context, log.rawLine].filter(Boolean).join(' ').toLowerCase();
        const matchesSearch = !q || haystack.includes(q);
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
