// 关键字高亮（WindTerm 那种）：不用装 zsh 插件，输出里的错误 / 警告 / 成功字样、IP、链接自动上色。
//
// 在写进 xterm 之前改输出流：只给「当前是默认样式」的纯文本段包上颜色码，包完立刻 `ESC[0m`
// 回到默认，所以不跟程序自己的颜色打架，光标位置也不变（颜色码不占格）。
// 跳过备用屏（vim / htop / less）、提示符和正在输入的命令行（有命令标记时才知道边界；
// 输入行在第一个换行处结束，PowerShell 那边没有「开始执行」的标记也能对上）。
// 一个词被拆在两块输出之间就不上色：不攒输出，回显不会慢。

type Mode = 'ground' | 'esc' | 'escInter' | 'csi' | 'osc' | 'oscEsc' | 'str' | 'strEsc';

const ESC = '\x1b';

// 样式位：全 0 才算默认样式
const FG = 1;
const BG = 1 << 1;
const BOLD = 1 << 2;
const DIM = 1 << 3;
const ITALIC = 1 << 4;
const UNDERLINE = 1 << 5;
const BLINK = 1 << 6;
const INVERSE = 1 << 7;
const HIDDEN = 1 << 8;
const STRIKE = 1 << 9;
const OVERLINE = 1 << 10;

const PATTERN = new RegExp(
    [
        String.raw`(?<url>\bhttps?:\/\/[^\s"'<>\x1b]*[^\s"'<>\x1b.,;:!?)\]}])`,
        String.raw`(?<err>\b(?:[Ee]rrors?|ERRORS?|[Ff]atal|FATAL|[Ff]ail(?:ed|ure|s)?|FAIL(?:ED|URE)?|[Ee]xception|Traceback|[Pp]anic(?:ked)?|[Dd]enied|[Rr]efused|[Ii]nvalid|INVALID|[Nn]ot found|NOT FOUND|[Cc]annot|[Uu]nable to|[Tt]imed out|[Ss]egmentation fault|[Kk]illed)\b|错误|失败|异常|报错|拒绝|无法|超时)`,
        String.raw`(?<warn>\b(?:[Ww]arn(?:ings?)?|WARN(?:INGS?)?|[Dd]eprecated|DEPRECATED)\b|警告|已弃用)`,
        String.raw`(?<ok>\b(?:[Ss]uccess(?:ful(?:ly)?)?|SUCCESS(?:FUL(?:LY)?)?|[Ss]ucceeded|OK|[Pp]assed|PASSED|[Dd]one|DONE|[Cc]ompleted?|COMPLETED?)\b|成功|完成)`,
        String.raw`(?<ip>\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?::\d{1,5})?\b)`,
        String.raw`(?<level>\b(?:INFO|DEBUG|TRACE)\b)`,
    ].join('|'),
    'g',
);

function colorOf(match: string, groups: Record<string, string | undefined>): string | null {
    if (groups.url) return '36';
    if (groups.err) return '31';
    if (groups.warn) return '33';
    if (groups.ok) return '32';
    if (groups.ip) return '35';
    if (groups.level) return match === 'INFO' ? '34' : '90';
    return null;
}

export function colorizeText(text: string): string {
    return text.replace(PATTERN, (...args: unknown[]) => {
        const match = args[0] as string;
        const groups = args[args.length - 1] as Record<string, string | undefined>;
        const color = colorOf(match, groups);
        return color ? `${ESC}[${color}m${match}${ESC}[0m` : match;
    });
}

/** 按 SGR 参数更新样式位 */
export function applySgr(style: number, params: string): number {
    if (params === '') return 0;
    const parts = params.split(/[;:]/).map((p) => (p === '' ? 0 : Number.parseInt(p, 10)));
    let s = style;
    for (let i = 0; i < parts.length; i++) {
        const p = parts[i] ?? 0;
        if (p === 0) s = 0;
        else if (p === 1) s |= BOLD;
        else if (p === 2) s |= DIM;
        else if (p === 3) s |= ITALIC;
        else if (p === 4 || p === 21) s |= UNDERLINE;
        else if (p === 5 || p === 6) s |= BLINK;
        else if (p === 7) s |= INVERSE;
        else if (p === 8) s |= HIDDEN;
        else if (p === 9) s |= STRIKE;
        else if (p === 22) s &= ~(BOLD | DIM);
        else if (p === 23) s &= ~ITALIC;
        else if (p === 24) s &= ~UNDERLINE;
        else if (p === 25) s &= ~BLINK;
        else if (p === 27) s &= ~INVERSE;
        else if (p === 28) s &= ~HIDDEN;
        else if (p === 29) s &= ~STRIKE;
        else if ((p >= 30 && p <= 37) || (p >= 90 && p <= 97)) s |= FG;
        else if (p === 39) s &= ~FG;
        else if ((p >= 40 && p <= 47) || (p >= 100 && p <= 107)) s |= BG;
        else if (p === 49) s &= ~BG;
        else if (p === 53) s |= OVERLINE;
        else if (p === 55) s &= ~OVERLINE;
        else if (p === 38 || p === 48 || p === 58) {
            if (p === 38) s |= FG;
            if (p === 48) s |= BG;
            // 38;5;n 或 38;2;r;g;b：跳过颜色参数
            const kind = parts[i + 1];
            i += kind === 5 ? 2 : kind === 2 ? 4 : 0;
        }
    }
    return s;
}

export interface KeywordHighlighter {
    process(chunk: string): string;
    reset(): void;
}

export function createKeywordHighlighter(): KeywordHighlighter {
    let mode: Mode = 'ground';
    let csi = '';
    let osc = '';
    let style = 0;
    let altScreen = false;
    let inPrompt = false;
    let inInput = false;
    let run = '';
    let out = '';

    const flush = () => {
        if (!run) return;
        out += style === 0 && !altScreen && !inPrompt && !inInput ? colorizeText(run) : run;
        run = '';
    };

    const onCsi = (body: string, final: string) => {
        if (final === 'm') {
            if (body === '' || /^[\d;:]+$/.test(body)) style = applySgr(style, body);
            return;
        }
        if ((final === 'h' || final === 'l') && body.startsWith('?')) {
            const modes = body.slice(1).split(';');
            if (modes.some((m) => m === '1049' || m === '1047' || m === '47')) {
                altScreen = final === 'h';
            }
        }
    };

    const onOsc = (body: string) => {
        const mark = body.startsWith('633;')
            ? body[4]
            : body.startsWith('133;')
              ? body[4]
              : undefined;
        if (mark === 'A') {
            inPrompt = true;
            inInput = false;
        } else if (mark === 'B') {
            inPrompt = false;
            inInput = true;
        } else if (mark === 'C' || mark === 'D') {
            inPrompt = false;
            inInput = false;
        }
    };

    const reset = () => {
        mode = 'ground';
        csi = '';
        osc = '';
        style = 0;
        altScreen = false;
        inPrompt = false;
        inInput = false;
        run = '';
    };

    const process = (chunk: string): string => {
        out = '';
        for (let i = 0; i < chunk.length; i++) {
            const c = chunk[i] as string;
            switch (mode) {
                case 'ground':
                    if (c === ESC) {
                        flush();
                        out += c;
                        mode = 'esc';
                    } else if (c === '\n' && inInput) {
                        flush();
                        inInput = false;
                        run += c;
                    } else {
                        run += c;
                    }
                    break;
                case 'esc':
                    out += c;
                    if (c === '[') {
                        mode = 'csi';
                        csi = '';
                    } else if (c === ']') {
                        mode = 'osc';
                        osc = '';
                    } else if (c === 'P' || c === 'X' || c === '^' || c === '_') {
                        mode = 'str';
                    } else if (c >= ' ' && c <= '/') {
                        mode = 'escInter';
                    } else {
                        if (c === 'c') {
                            style = 0;
                            altScreen = false;
                        }
                        mode = 'ground';
                    }
                    break;
                case 'escInter':
                    out += c;
                    if (!(c >= ' ' && c <= '/')) mode = 'ground';
                    break;
                case 'csi':
                    out += c;
                    if (c >= '@' && c <= '~') {
                        onCsi(csi, c);
                        mode = 'ground';
                    } else if (csi.length < 64) {
                        csi += c;
                    }
                    break;
                case 'osc':
                    out += c;
                    if (c === '\x07') {
                        onOsc(osc);
                        mode = 'ground';
                    } else if (c === ESC) {
                        mode = 'oscEsc';
                    } else if (osc.length < 16) {
                        osc += c;
                    }
                    break;
                case 'oscEsc':
                    onOsc(osc);
                    if (c === '\\') {
                        out += c;
                        mode = 'ground';
                    } else {
                        // ESC 后面不是 `\`：OSC 到此为止，这个字符按新的转义序列重新读
                        mode = 'esc';
                        i--;
                    }
                    break;
                case 'str':
                    out += c;
                    if (c === ESC) mode = 'strEsc';
                    else if (c === '\x07') mode = 'ground';
                    break;
                case 'strEsc':
                    if (c === '\\') {
                        out += c;
                        mode = 'ground';
                    } else {
                        mode = 'esc';
                        i--;
                    }
                    break;
            }
        }
        flush();
        return out;
    };

    return { process, reset };
}
