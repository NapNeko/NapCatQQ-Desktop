// Bot 日志环形缓冲。纯数据 + 纯函数。
//
// 历史快照（一次性）+ 增量 `bot_log_appended` / `snowluma_daemon_log` 双源。
// 上限 1000 行防止内存膨胀；超过时丢最早的。
//
// LogLevel 由本模块按行内容解析出来（trace / debug / info / success / warn /
// error / fatal / unknown），UI 只负责按 level 上颜色，不再做正则识别。
// 上游带的终端颜色码解析成 spans 留给界面画；时间、等级挪进单独的列，正文里挖掉。

import { cutAnsi, parseAnsi, type AnsiSpan, type AnsiText } from './ansi';

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
    /** 正文里带颜色的段（text 的偏移），来自上游输出的终端颜色码 */
    spans?: AnsiSpan[];
    /** Koishi 日志的来源名（loader / app / 插件名），从正文挖出来在界面上单独画 */
    scope?: string;
    /** 没有自己的时间和等级，接着上一行（堆栈、多行输出）：等级跟上一行，界面不再重复时间和标签 */
    continuation?: boolean;
}

const MAX_LINES = 1000;

let counter = 0;
function nextId(prefix = 'log'): string {
    counter += 1;
    return `${prefix}-${Date.now()}-${counter}`;
}

// Karin chalk：`[Karin][20:16:22.716][MARK]`。MARK 是灰标，对到 trace。
// 麦麦 full 样式补空格对齐：`[    INFO]`、`[CRITICAL]`
const BRACKET_LEVEL_PATTERN =
    /\[\s*(trace|debug|info|warn|warning|error|fatal|critical|success|mark)\s*\]/i;

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
    const tagged = taggedLevel(cleaned);
    return tagged === 'unknown' ? standaloneLevel(cleaned) : tagged;
}

/** 行里明写的等级：`[INFO]`、`| INFO |` 或 Koishi 的 `时间 [I]` */
function taggedLevel(cleaned: string): LogLevel {
    const koishi = cleaned.match(KOISHI_TS_PREFIX);
    if (koishi) return KOISHI_LEVEL_LETTER[koishi[2]] ?? 'unknown';
    const bracket = cleaned.match(BRACKET_LEVEL_PATTERN);
    if (bracket) {
        return normalizeLevel(bracket[1]);
    }
    const pipe = cleaned.match(PIPE_LEVEL_PATTERN);
    if (pipe) {
        return normalizeLevel(pipe[1]);
    }
    return 'unknown';
}

function standaloneLevel(cleaned: string): LogLevel {
    const standalone = cleaned.match(STANDALONE_LEVEL_PATTERN);
    return standalone ? normalizeLevel(standalone[1]) : 'unknown';
}

// 麦麦默认的 lite 样式不写等级，只给时间戳上色（上游 ModuleColoredConsoleRenderer）：
// 天蓝 117 info、黄 warn、红 error、橙 208 debug、绿 success、紫 critical
const STAMP_COLOR_LEVEL: Record<number, LogLevel> = {
    117: 'info',
    3: 'warn',
    11: 'warn',
    1: 'error',
    9: 'error',
    208: 'debug',
    2: 'success',
    10: 'success',
    5: 'fatal',
    13: 'fatal',
};

function stampColorLevel(parsed: AnsiText, stampEnd: number): LogLevel | null {
    const first = parsed.spans[0];
    if (!first || first.start !== 0 || first.end < stampEnd) return null;
    const fg = first.style.fg;
    if (fg?.kind !== 'palette') return null;
    return STAMP_COLOR_LEVEL[fg.index] ?? null;
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
        case 'critical':
            return 'fatal';
        default:
            return 'unknown';
    }
}


// NapCat 控制台: `07-11 17:06:19 [info] nick | msg`（sanitize 后）；麦麦也是 `月-日 时:分:秒` 打头
// 捕获组: 月日 / 时分秒 / 正文
const NAPCAT_TS_PREFIX =
    /^(\d{1,2}-\d{1,2})\s+(\d{1,2}:\d{2}:\d{2})\s+(.*)$/;
const NAPCAT_STAMP = /^\d{1,2}-\d{1,2}\s+\d{1,2}:\d{2}:\d{2}/;

// Koishi: `2026-09-29 21:22:14 [I] loader apply plugin …`，全年月日 + 单字母等级 + 来源名打头
const KOISHI_TS_PREFIX =
    /^\d{4}-\d{2}-\d{2}\s+(\d{1,2}:\d{2}:\d{2})\s+\[([DIWES])\]\s+(.*)$/;

const KOISHI_LEVEL_LETTER: Record<string, LogLevel> = {
    D: 'debug',
    I: 'info',
    W: 'warn',
    E: 'error',
    S: 'success',
};

// SnowLuma daemon: 22:21:24 INFO               [App] ...（无月日）
const SNOWLUMA_TS_PREFIX =
    /^(\d{1,2}:\d{2}:\d{2})\s+(INFO|WARN|WARNING|ERROR|DEBUG|TRACE|OK|FATAL)?\s*(.*)$/i;

// 方括号包着的时间打头：AstrBot `[18:55:54.222] [Core] [INFO] …`，
// 它自带的 hypercorn `[2026-09-27 10:42:49 +0800] [20504] [INFO] …`
const BRACKET_TS_PREFIX =
    /^\[(?:\d{4}-\d{2}-\d{2}[ T])?(\d{1,2}:\d{2}:\d{2})(?:[.,]\d+)?(?:\s*[+-]\d{2}:?\d{2})?\]\s*/;

// 裸 ISO 时间打头：`2026-10-04 03:41:22.123 | INFO | 模块 | 正文`。
// loguru 的 `{time:YYYY-MM-DD HH:mm:ss.SSS}` 就是这个形状；NeoBot 用它，
// 而且**不算时间戳**的话整行都会挤进正文列，时间和模块名全糊在消息里。
const ISO_TS_PREFIX =
    /^(\d{4}-\d{2}-\d{2})[ T](\d{1,2}:\d{2}:\d{2})(?:[.,]\d+)?(?:\s*[+-]\d{2}:?\d{2})?\s*/;

// 时间后面几组方括号里的等级：`[info] nick |`、`[Core] [INFO] [模块:行]:`、`[20504] [INFO]`、`[    INFO]`。
// UI 已有 INF/ERR 列，正文里再留一份是重复
const LEADING_GROUP_LEVEL =
    /^((?:\[[^\]\n]*\]\s*){0,4}?)\[\s*(?:trace|debug|info|warn|warning|error|fatal|critical|success|mark)\s*\]\s*/i;

// 竖线分隔的等级标签：`| INFO |`、`| SUCCESS |`。loguru 的
// `{time} | {level} | {name} | {message}` 走这条——UI 已有级别列，正文里再留一份是重复。
// 和方括号版一样，只吃补位空格，保留标签后真正的正文分隔空格。
const LEADING_PIPE_LEVEL =
    /^\|\s*(?:trace|debug|info|warn|warning|error|fatal|critical|success|mark)\s*\|/i;

// `[Karin][20:16:22.716][INFO] body` — 时分秒给时间列，LEVEL 给色条，信封不进正文
const FRAMEWORK_TS_LEVEL =
    /^\[([^\]]+)\]\[(\d{1,2}:\d{2}:\d{2})(?:\.\d+)?\]\[([A-Za-z]+)\]\s*(.*)$/;

/** 列表时间列只放 HH:mm:ss；整段 `MM-DD HH:mm:ss` 塞 20px 行会折成只剩日期 */
function normalizeClock(ts: string): string {
    const m = ts.match(/(\d{1,2}):(\d{2}):(\d{2})/);
    if (!m) return ts;
    return `${m[1].padStart(2, '0')}:${m[2]}:${m[3]}`;
}

interface LineSplit {
    timestamp: string;
    /** 行里自带时间的格式；none 是没带，时间用 fallback */
    kind: 'framework' | 'bracket' | 'iso' | 'napcat' | 'snowluma' | 'koishi' | 'none';
    /** 时间戳在原文里结束的位置，看它的颜色判等级用 */
    stampEnd: number;
    /** 从正文挖掉的区间：时间前缀、等级标签 */
    cuts: Array<[number, number]>;
    /** Koishi 行自带的单字母等级（D/I/W/E/S），已在别处识别出就不设 */
    level?: LogLevel;
    /** Koishi 行的来源名（loader / app / 插件名），从正文里挖出来单独画 */
    scope?: string;
}

function splitLine(cleaned: string, fallbackTs: string): LineSplit {
    const fw = cleaned.match(FRAMEWORK_TS_LEVEL);
    if (fw) {
        const bodyStart = cleaned.length - fw[4].length;
        return withLevelCut(cleaned, bodyStart, {
            timestamp: normalizeClock(fw[2]),
            kind: 'framework',
            stampEnd: bodyStart,
            cuts: [[0, bodyStart]],
        });
    }
    const ko = cleaned.match(KOISHI_TS_PREFIX);
    if (ko) {
        const bodyStart = cleaned.length - ko[3].length;
        // 来源名跟着等级标签：`[I] loader apply …` → scope = loader，连着后面的空格一起挖掉
        const scopeMatch = ko[3].match(/^(\S+)(?:\s+|$)/);
        const scope = scopeMatch?.[1];
        const cuts: Array<[number, number]> = [[0, bodyStart]];
        if (scope) cuts.push([bodyStart, bodyStart + scopeMatch[0].length]);
        return {
            timestamp: normalizeClock(ko[1]),
            kind: 'koishi',
            stampEnd: bodyStart,
            cuts,
            level: KOISHI_LEVEL_LETTER[ko[2]],
            scope,
        };
    }
    const br = cleaned.match(BRACKET_TS_PREFIX);
    if (br) {
        return withLevelCut(cleaned, br[0].length, {
            timestamp: normalizeClock(br[1]),
            kind: 'bracket',
            stampEnd: br[0].trimEnd().length,
            cuts: [[0, br[0].length]],
        });
    }
    const iso = cleaned.match(ISO_TS_PREFIX);
    if (iso) {
        return withLevelCut(cleaned, iso[0].length, {
            timestamp: normalizeClock(iso[2]),
            kind: 'iso',
            stampEnd: iso[0].trimEnd().length,
            cuts: [[0, iso[0].length]],
        });
    }
    const m = cleaned.match(NAPCAT_TS_PREFIX);
    if (m) {
        const bodyStart = cleaned.length - m[3].length;
        return withLevelCut(cleaned, bodyStart, {
            timestamp: normalizeClock(m[2]),
            kind: 'napcat',
            stampEnd: cleaned.match(NAPCAT_STAMP)?.[0].length ?? bodyStart,
            cuts: [[0, bodyStart]],
        });
    }
    const sl = cleaned.match(SNOWLUMA_TS_PREFIX);
    if (sl) {
        const rest = (sl[3] ?? '').trimStart();
        // 时间后面什么都没有时整行照旧当正文
        const bodyStart = rest ? cleaned.length - rest.length : 0;
        return withLevelCut(cleaned, bodyStart, {
            timestamp: normalizeClock(sl[1]),
            kind: 'snowluma',
            stampEnd: sl[1].length,
            cuts: bodyStart ? [[0, bodyStart]] : [],
        });
    }
    return withLevelCut(cleaned, 0, {
        timestamp: normalizeClock(fallbackTs),
        kind: 'none',
        stampEnd: 0,
        cuts: [],
    });
}

function withLevelCut(cleaned: string, bodyStart: number, split: LineSplit): LineSplit {
    // 竖线版：整段 `| INFO |` 都是信封，连它自己的空格一起挖掉。
    // 再多吃掉后的前导空白：loguru 的 `{level: <8}` 会在等级右边补空格，
    // 不补位（WARNING）和补位（INFO    ）两种宽度不同，留着正文就对不齐了。
    // 等级已有独立列，这点对齐空格属于信封。
    const pipe = cleaned.slice(bodyStart).match(LEADING_PIPE_LEVEL);
    if (pipe) {
        const after = bodyStart + pipe[0].length;
        const indent = cleaned.slice(after).match(/^[ \t]*/)?.[0].length ?? 0;
        return {
            ...split,
            cuts: [...split.cuts, [bodyStart, after + indent]],
        };
    }
    const m = cleaned.slice(bodyStart).match(LEADING_GROUP_LEVEL);
    if (!m) return split;
    const lead = m[1];
    // `[Karin][INFO] x` 标签前没有空格：只挖标签，后面的空格留着把两边隔开
    const tagEnd = lead && !/\s$/.test(lead) ? m[0].trimEnd().length : m[0].length;
    return { ...split, cuts: [...split.cuts, [bodyStart + lead.length, bodyStart + tagEnd]] };
}

/** 拆时间前缀和等级标签；行里没带时间时 ts 用 fallback */
export function splitLogTimestamp(
    line: string,
    fallbackTs: string,
): { timestamp: string; body: string } {
    const cleaned = stripAnsiEscapes(line);
    const split = splitLine(cleaned, fallbackTs);
    return {
        timestamp: split.timestamp,
        body: cutAnsi({ text: cleaned, spans: [] }, split.cuts).text,
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

/**
 * 一行原始输出 → 一条日志。等级先看行里明写的，没写再看麦麦那种时间戳颜色，最后才猜单词。
 * 既没时间也没等级的行（堆栈、多行输出的后几行）算上一条的续行，等级跟上一条走，
 * 按错误筛的时候整段堆栈一起出来
 */
function buildEntry(
    line: string,
    channel: LogChannel,
    fallbackTs: string,
    prev: LogEntry | undefined,
    id: string,
): LogEntry | null {
    const parsed = parseAnsi(line);
    const cleaned = parsed.text;
    if (!cleaned.trim()) return null;
    const split = splitLine(cleaned, fallbackTs);
    const body = cutAnsi(parsed, split.cuts);
    let level = split.level ?? taggedLevel(cleaned);
    if (level === 'unknown' && split.kind === 'napcat') {
        level = stampColorLevel(parsed, split.stampEnd) ?? 'unknown';
    }
    if (level === 'unknown') level = standaloneLevel(cleaned);
    const continuation = level === 'unknown' && split.kind === 'none' && !!prev;
    const entry: LogEntry = {
        id,
        text: body.text,
        channel,
        level: continuation && prev ? prev.level : level,
        timestamp: split.timestamp,
    };
    if (body.spans.length) entry.spans = body.spans;
    if (split.scope) entry.scope = split.scope;
    if (continuation) entry.continuation = true;
    return entry;
}

/** 盘上历史里没带时间的行不知道是什么时候写的：时间留空，不拿打开页面的时刻充数 */
export function buildHistoryEntries(lines: string[], now = ''): LogEntry[] {
    const out: LogEntry[] = [];
    for (let idx = 0; idx < lines.length; idx++) {
        const raw = lines[idx];
        if (!raw) continue;
        const entry = buildEntry(raw, 'unknown', now, out[out.length - 1], `hist-${idx}-${counter++}`);
        if (entry) out.push(entry);
    }
    return out;
}

export function appendLine(
    logs: LogEntry[],
    line: string,
    channel: LogChannel = 'stdout',
    now = new Date().toLocaleTimeString(),
): LogEntry[] {
    const entry = buildEntry(line, channel, now, logs[logs.length - 1], nextId());
    if (!entry) return logs;
    const next =
        logs.length >= MAX_LINES ? logs.slice(logs.length - MAX_LINES + 1) : logs.slice();
    next.push(entry);
    return next;
}

/// 会话里已经进过缓冲的脏行（未剥 ANSI / 还带着 Karin 信封）再洗一遍。
export function canonicalizeLogEntry(entry: LogEntry): LogEntry {
    // 带样式段的是按现在的规则建的，偏移对着 text，不能再动
    if (entry.spans) return entry;
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
        const haystack = [log.text, log.context, log.scope, log.rawLine].filter(Boolean).join(' ').toLowerCase();
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
