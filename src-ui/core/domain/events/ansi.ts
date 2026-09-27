// 终端颜色码（SGR，`ESC [ … m`）→ 纯文本 + 按偏移标出的样式段。
//
// 只认 SGR：光标移动、标题（OSC）这类控制序列和 C0 控制符都丢掉，丢法和 stripAnsiEscapes 一样。
// 回车按终端的意思处理：`\r` 之后的内容盖掉前面的（进度条一行刷十几次，只留最后一次）；
// 最后一段是空的就留前一段。背景色不画，日志面板自己有底色。

export type AnsiColor =
    | { kind: 'palette'; index: number }
    | { kind: 'rgb'; r: number; g: number; b: number };

export interface AnsiStyle {
    fg?: AnsiColor;
    bold?: boolean;
    dim?: boolean;
    italic?: boolean;
    underline?: boolean;
}

/** `[start, end)` 是纯文本里的偏移 */
export interface AnsiSpan {
    start: number;
    end: number;
    style: AnsiStyle;
}

export interface AnsiText {
    text: string;
    /** 只有带样式的段；没标到的就是默认样式 */
    spans: AnsiSpan[];
}

export function parseAnsi(input: string): AnsiText {
    let style: AnsiStyle = {};
    let text = '';
    let spans: AnsiSpan[] = [];
    let kept: AnsiText | null = null;
    let buf = '';

    const flush = () => {
        if (!buf) return;
        const start = text.length;
        text += buf;
        buf = '';
        if (isPlain(style)) return;
        const last = spans[spans.length - 1];
        if (last && last.end === start && sameStyle(last.style, style)) {
            last.end = text.length;
        } else {
            spans.push({ start, end: text.length, style: { ...style } });
        }
    };

    const n = input.length;
    let i = 0;
    while (i < n) {
        const c = input.charCodeAt(i);
        if (c === 0x1b || c === 0x9b) {
            flush();
            const next = c === 0x1b && i + 1 < n ? input.charCodeAt(i + 1) : 0;
            if (c === 0x9b || next === 0x5b) {
                const csi = readCsi(input, c === 0x9b ? i + 1 : i + 2);
                if (csi.final === 0x6d) style = applySgr(style, csi.params);
                i = csi.end;
                continue;
            }
            if (next === 0x5d || next === 0x50 || next === 0x58 || next === 0x5e || next === 0x5f) {
                i = consumeStringSeq(input, i + 2);
                continue;
            }
            i += next ? 2 : 1;
            continue;
        }
        if (c === 0x0d) {
            flush();
            if (text.trim()) kept = { text, spans };
            text = '';
            spans = [];
            i += 1;
            continue;
        }
        if (c === 0x7f || (c < 0x20 && c !== 0x09 && c !== 0x0a)) {
            i += 1;
            continue;
        }
        buf += input[i];
        i += 1;
    }
    flush();
    if (!text.trim() && kept) return kept;
    return { text, spans };
}

/**
 * 从纯文本里挖掉几段（挪去单独一列的时间、等级标签），样式段跟着挪。
 * 区间是原文偏移，可以乱序、可以重叠。
 */
export function cutAnsi(src: AnsiText, cuts: ReadonlyArray<readonly [number, number]>): AnsiText {
    const merged = mergeRanges(cuts, src.text.length);
    if (merged.length === 0) return src;
    const keep: Array<[number, number]> = [];
    let at = 0;
    for (const [a, b] of merged) {
        if (a > at) keep.push([at, a]);
        at = Math.max(at, b);
    }
    if (at < src.text.length) keep.push([at, src.text.length]);

    let text = '';
    const offsets: number[] = [];
    for (const [a, b] of keep) {
        offsets.push(text.length - a);
        text += src.text.slice(a, b);
    }
    const spans: AnsiSpan[] = [];
    for (const span of src.spans) {
        keep.forEach(([a, b], k) => {
            const start = Math.max(a, span.start);
            const end = Math.min(b, span.end);
            if (start >= end) return;
            const shifted = { start: start + offsets[k], end: end + offsets[k], style: span.style };
            const last = spans[spans.length - 1];
            if (last && last.end === shifted.start && last.style === span.style) last.end = shifted.end;
            else spans.push(shifted);
        });
    }
    return { text, spans };
}

function mergeRanges(
    cuts: ReadonlyArray<readonly [number, number]>,
    len: number,
): Array<[number, number]> {
    const sorted = cuts
        .map(([a, b]) => [Math.max(0, a), Math.min(len, b)] as [number, number])
        .filter(([a, b]) => b > a)
        .sort((x, y) => x[0] - y[0]);
    const out: Array<[number, number]> = [];
    for (const r of sorted) {
        const last = out[out.length - 1];
        if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
        else out.push([r[0], r[1]]);
    }
    return out;
}

function isPlain(s: AnsiStyle): boolean {
    return !s.fg && !s.bold && !s.dim && !s.italic && !s.underline;
}

function sameColor(a?: AnsiColor, b?: AnsiColor): boolean {
    if (!a || !b) return a === b;
    if (a.kind === 'palette' && b.kind === 'palette') return a.index === b.index;
    if (a.kind === 'rgb' && b.kind === 'rgb') return a.r === b.r && a.g === b.g && a.b === b.b;
    return false;
}

function sameStyle(a: AnsiStyle, b: AnsiStyle): boolean {
    return (
        sameColor(a.fg, b.fg) &&
        !!a.bold === !!b.bold &&
        !!a.dim === !!b.dim &&
        !!a.italic === !!b.italic &&
        !!a.underline === !!b.underline
    );
}

/** 参数字节 / 中间字节 / 结束字节，和 stripAnsiEscapes 的 consumeCsi 同一套边界 */
function readCsi(input: string, start: number): { end: number; params: string; final: number } {
    let i = start;
    const n = input.length;
    while (i < n && input.charCodeAt(i) >= 0x30 && input.charCodeAt(i) <= 0x3f) i += 1;
    const params = input.slice(start, i);
    while (i < n && input.charCodeAt(i) >= 0x20 && input.charCodeAt(i) <= 0x2f) i += 1;
    let final = 0;
    if (i < n && input.charCodeAt(i) >= 0x40 && input.charCodeAt(i) <= 0x7e) {
        final = input.charCodeAt(i);
        i += 1;
    }
    return { end: i, params, final };
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

function byte(v: number | undefined): number {
    return Math.max(0, Math.min(255, v ?? 0));
}

/** 38 / 48 后面的扩展色：`5;n` 调色板，`2;r;g;b` 真彩。返回颜色和吃掉了几个参数 */
function extendedColor(rest: number[]): { color?: AnsiColor; used: number } {
    if (rest[0] === 5) return { color: { kind: 'palette', index: byte(rest[1]) }, used: 2 };
    if (rest[0] === 2) {
        return { color: { kind: 'rgb', r: byte(rest[1]), g: byte(rest[2]), b: byte(rest[3]) }, used: 4 };
    }
    return { used: rest.length };
}

function applySgr(prev: AnsiStyle, raw: string): AnsiStyle {
    // `?` `<` 这类私有参数不是 SGR
    if (/[^0-9;:]/.test(raw)) return prev;
    let s: AnsiStyle = { ...prev };
    const groups = raw === '' ? ['0'] : raw.split(';');
    for (let k = 0; k < groups.length; k++) {
        const group = groups[k];
        // 冒号写法 `38:2::255:0:0` / `38:5:208`：一组自带全部参数
        if (group.includes(':')) {
            const parts = group.split(':').map((p) => (p === '' ? -1 : Number(p)));
            if (parts[0] === 38 || parts[0] === 48) {
                const rest = parts.slice(1);
                // 真彩的冒号写法中间多一个色彩空间 id（常空着）
                const args = rest[0] === 2 && rest.length >= 5 ? [2, ...rest.slice(2)] : rest;
                const { color } = extendedColor(args.map((v) => (v < 0 ? 0 : v)));
                if (parts[0] === 38 && color) s.fg = color;
            } else if (parts[0] === 4) {
                s.underline = parts[1] !== 0;
            }
            continue;
        }
        const p = group === '' ? 0 : Number(group);
        if (p === 38 || p === 48) {
            const rest = groups.slice(k + 1).map((g) => (g === '' ? 0 : Number(g)));
            const { color, used } = extendedColor(rest);
            if (p === 38 && color) s.fg = color;
            k += used;
            continue;
        }
        if (p === 0) s = {};
        else if (p === 1) s.bold = true;
        else if (p === 2) s.dim = true;
        else if (p === 3) s.italic = true;
        else if (p === 4) s.underline = true;
        else if (p === 22) {
            s.bold = false;
            s.dim = false;
        } else if (p === 23) s.italic = false;
        else if (p === 24) s.underline = false;
        else if (p >= 30 && p <= 37) s.fg = { kind: 'palette', index: p - 30 };
        else if (p === 39) delete s.fg;
        else if (p >= 90 && p <= 97) s.fg = { kind: 'palette', index: p - 90 + 8 };
    }
    for (const key of ['bold', 'dim', 'italic', 'underline'] as const) {
        if (!s[key]) delete s[key];
    }
    return s;
}
