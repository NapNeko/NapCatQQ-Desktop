// 配置原文分词：给 CodeMirror 着色用。拼回去必须等于原文。

export type SyntaxMode = 'json' | 'dot_env' | 'toml' | 'yaml' | 'plain' | 'prompt';

export type TokKind =
    | 'key'
    | 'string'
    | 'number'
    | 'bool'
    | 'null'
    | 'punct'
    | 'comment'
    | 'space'
    | 'plain'
    | 'param';

export interface Tok {
    kind: TokKind;
    text: string;
}

function tokenizeJson(source: string): Tok[] {
    const out: Tok[] = [];
    let i = 0;
    const n = source.length;
    const push = (kind: TokKind, text: string) => {
        if (!text) return;
        const last = out[out.length - 1];
        if (last && last.kind === kind) last.text += text;
        else out.push({ kind, text });
    };

    while (i < n) {
        const ch = source[i]!;
        if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r') {
            let j = i + 1;
            while (j < n) {
                const c = source[j]!;
                if (c !== ' ' && c !== '\t' && c !== '\n' && c !== '\r') break;
                j += 1;
            }
            push('space', source.slice(i, j));
            i = j;
            continue;
        }
        if (ch === '"') {
            let j = i + 1;
            let escaped = false;
            while (j < n) {
                const c = source[j]!;
                if (escaped) {
                    escaped = false;
                    j += 1;
                    continue;
                }
                if (c === '\\') {
                    escaped = true;
                    j += 1;
                    continue;
                }
                if (c === '"') {
                    j += 1;
                    break;
                }
                j += 1;
            }
            const lit = source.slice(i, j);
            let k = j;
            while (k < n && (source[k] === ' ' || source[k] === '\t')) k += 1;
            push(source[k] === ':' ? 'key' : 'string', lit);
            i = j;
            continue;
        }
        if (ch === '-' || (ch >= '0' && ch <= '9')) {
            let j = i + 1;
            while (j < n) {
                const c = source[j]!;
                if ((c >= '0' && c <= '9') || c === '.' || c === 'e' || c === 'E' || c === '+' || c === '-') {
                    j += 1;
                    continue;
                }
                break;
            }
            push('number', source.slice(i, j));
            i = j;
            continue;
        }
        if (source.startsWith('true', i)) {
            push('bool', 'true');
            i += 4;
            continue;
        }
        if (source.startsWith('false', i)) {
            push('bool', 'false');
            i += 5;
            continue;
        }
        if (source.startsWith('null', i)) {
            push('null', 'null');
            i += 4;
            continue;
        }
        if ('{}[],:'.includes(ch)) {
            push('punct', ch);
            i += 1;
            continue;
        }
        push('plain', ch);
        i += 1;
    }
    return out;
}

function tokenizeDotenv(source: string): Tok[] {
    const out: Tok[] = [];
    const push = (kind: TokKind, text: string) => {
        if (!text) return;
        out.push({ kind, text });
    };

    let i = 0;
    const n = source.length;
    while (i < n) {
        if (source[i] === '\n' || source[i] === '\r') {
            const j = source[i] === '\r' && source[i + 1] === '\n' ? i + 2 : i + 1;
            push('space', source.slice(i, j));
            i = j;
            continue;
        }
        let ws = i;
        while (ws < n && (source[ws] === ' ' || source[ws] === '\t')) ws += 1;
        if (ws > i) {
            push('space', source.slice(i, ws));
            i = ws;
        }
        if (i >= n) break;
        if (source[i] === '#') {
            let j = i + 1;
            while (j < n && source[j] !== '\n' && source[j] !== '\r') j += 1;
            push('comment', source.slice(i, j));
            i = j;
            continue;
        }
        let eq = i;
        while (eq < n && source[eq] !== '=' && source[eq] !== '\n' && source[eq] !== '\r' && source[eq] !== '#') {
            eq += 1;
        }
        if (eq < n && source[eq] === '=') {
            push('key', source.slice(i, eq));
            push('punct', '=');
            i = eq + 1;
            const quote = source[i];
            if (quote === '"' || quote === "'") {
                let j = i + 1;
                while (j < n && source[j] !== quote && source[j] !== '\n' && source[j] !== '\r') j += 1;
                if (j < n && source[j] === quote) j += 1;
                push('string', source.slice(i, j));
                i = j;
            } else {
                let j = i;
                while (j < n && source[j] !== '\n' && source[j] !== '\r' && source[j] !== '#') j += 1;
                push('plain', source.slice(i, j));
                i = j;
            }
            continue;
        }
        let j = i;
        while (j < n && source[j] !== '\n' && source[j] !== '\r') j += 1;
        push('plain', source.slice(i, j));
        i = j;
    }
    return out;
}

function tokenizeToml(source: string): Tok[] {
    const out: Tok[] = [];
    const push = (kind: TokKind, text: string) => {
        if (!text) return;
        out.push({ kind, text });
    };
    let i = 0;
    const n = source.length;
    while (i < n) {
        const ch = source[i]!;
        if (ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r') {
            let j = i + 1;
            while (j < n && ' \t\n\r'.includes(source[j]!)) j += 1;
            push('space', source.slice(i, j));
            i = j;
            continue;
        }
        if (ch === '#') {
            let j = i + 1;
            while (j < n && source[j] !== '\n' && source[j] !== '\r') j += 1;
            push('comment', source.slice(i, j));
            i = j;
            continue;
        }
        if (ch === '"' || ch === "'") {
            let j = i + 1;
            while (j < n && source[j] !== ch && source[j] !== '\n') j += 1;
            if (j < n && source[j] === ch) j += 1;
            push('string', source.slice(i, j));
            i = j;
            continue;
        }
        if (ch === '[') {
            let j = i + 1;
            while (j < n && source[j] !== ']' && source[j] !== '\n') j += 1;
            if (j < n && source[j] === ']') j += 1;
            push('key', source.slice(i, j));
            i = j;
            continue;
        }
        if (ch === '-' || (ch >= '0' && ch <= '9')) {
            let j = i + 1;
            while (j < n && /[0-9._eE+-]/.test(source[j]!)) j += 1;
            push('number', source.slice(i, j));
            i = j;
            continue;
        }
        if (source.startsWith('true', i) || source.startsWith('false', i)) {
            const lit = source.startsWith('true', i) ? 'true' : 'false';
            push('bool', lit);
            i += lit.length;
            continue;
        }
        if ('={},'.includes(ch)) {
            push('punct', ch);
            i += 1;
            continue;
        }
        let j = i + 1;
        while (j < n && /[A-Za-z0-9_\-]/.test(source[j]!)) j += 1;
        const word = source.slice(i, j);
        let k = j;
        while (k < n && (source[k] === ' ' || source[k] === '\t')) k += 1;
        push(source[k] === '=' ? 'key' : 'plain', word);
        i = j;
    }
    return out;
}

const YAML_BOOL = new Set(['true', 'false', 'True', 'False', 'TRUE', 'FALSE']);
const YAML_NULL = new Set(['null', 'Null', 'NULL', '~']);

/** 值里的 ` #` 起是行尾注释；引号里的 # 不算 */
function yamlCommentAt(line: string, from: number): number {
    let quote: string | null = null;
    for (let i = from; i < line.length; i += 1) {
        const c = line[i]!;
        if (quote) {
            if (c === quote) quote = null;
            continue;
        }
        if (c === '"' || c === "'") quote = c;
        else if (c === '#' && (i === from || line[i - 1] === ' ' || line[i - 1] === '\t')) return i;
    }
    return -1;
}

/** 映射键的冒号：后面是空白或行尾才算（`http://x` 里的不算） */
function yamlKeyColon(line: string, from: number): number {
    if (line[from] === '"' || line[from] === "'") {
        const close = line.indexOf(line[from]!, from + 1);
        if (close > from && line[close + 1] === ':') return close + 1;
        return -1;
    }
    for (let i = from; i < line.length; i += 1) {
        const c = line[i]!;
        if (c === '#' && i > from && line[i - 1] === ' ') return -1;
        if (c === ':' && (i + 1 === line.length || line[i + 1] === ' ' || line[i + 1] === '\t')) return i;
    }
    return -1;
}

function yamlScalarKind(value: string): TokKind {
    const t = value.trim();
    if (t.startsWith('"') || t.startsWith("'")) return 'string';
    if (YAML_BOOL.has(t)) return 'bool';
    if (YAML_NULL.has(t)) return 'null';
    if (/^[-+]?(\d[\d_]*)(\.\d+)?([eE][-+]?\d+)?$/.test(t) || /^0x[0-9a-fA-F]+$/.test(t)) return 'number';
    return 'plain';
}

// 云崽 config 那几份的样子：按行切，缩进 / 列表的 `- ` / 键 / 值 / 行尾注释。块标量和流式集合不细分，照普通文本画
function tokenizeYaml(source: string): Tok[] {
    const out: Tok[] = [];
    const push = (kind: TokKind, text: string) => {
        if (!text) return;
        const last = out[out.length - 1];
        if (last && last.kind === kind) last.text += text;
        else out.push({ kind, text });
    };
    const lines = source.split(/(\r?\n)/);
    for (const line of lines) {
        if (line === '\n' || line === '\r\n') {
            push('space', line);
            continue;
        }
        let i = 0;
        while (i < line.length && (line[i] === ' ' || line[i] === '\t')) i += 1;
        push('space', line.slice(0, i));
        if (line[i] === '#') {
            push('comment', line.slice(i));
            continue;
        }
        if (line.startsWith('---', i) || line.startsWith('...', i)) {
            push('punct', line.slice(i));
            continue;
        }
        while (line[i] === '-' && (i + 1 === line.length || line[i + 1] === ' ')) {
            push('punct', '-');
            let j = i + 1;
            while (j < line.length && line[j] === ' ') j += 1;
            push('space', line.slice(i + 1, j));
            i = j;
        }
        const colon = yamlKeyColon(line, i);
        if (colon >= 0) {
            push('key', line.slice(i, colon));
            push('punct', ':');
            i = colon + 1;
        }
        const hash = yamlCommentAt(line, i);
        const valueEnd = hash >= 0 ? hash : line.length;
        const value = line.slice(i, valueEnd);
        const lead = value.length - value.trimStart().length;
        const trail = value.length - value.trimEnd().length;
        push('space', value.slice(0, lead));
        push(yamlScalarKind(value), value.slice(lead, value.length - trail));
        push('space', value.slice(value.length - trail));
        if (hash >= 0) push('comment', line.slice(hash));
    }
    return out;
}

// 提示词模板（Python str.format）：{name} 是参数，{{ }} 是字面括号。没配对的括号照普通字符画，
// 错在哪由调用方的校验去说，这里不跟着报
function tokenizePrompt(source: string): Tok[] {
    const out: Tok[] = [];
    const push = (kind: TokKind, text: string) => {
        if (!text) return;
        const last = out[out.length - 1];
        if (last && last.kind === kind && kind !== 'param') last.text += text;
        else out.push({ kind, text });
    };
    let i = 0;
    while (i < source.length) {
        const c = source[i];
        const n = source[i + 1];
        if ((c === '{' && n === '{') || (c === '}' && n === '}')) {
            push('punct', c + n);
            i += 2;
            continue;
        }
        if (c === '{') {
            const close = source.indexOf('}', i + 1);
            const reopen = source.indexOf('{', i + 1);
            const body = close > i ? source.slice(i + 1, close) : '';
            if (close > i && (reopen === -1 || reopen > close) && !body.includes('\n')) {
                push('param', source.slice(i, close + 1));
                i = close + 1;
                continue;
            }
        }
        let j = i + 1;
        while (j < source.length && source[j] !== '{' && source[j] !== '}') j += 1;
        push('plain', source.slice(i, j));
        i = j;
    }
    return out;
}

export function tokenize(source: string, mode: SyntaxMode): Tok[] {
    switch (mode) {
        case 'json':
            return tokenizeJson(source);
        case 'dot_env':
            return tokenizeDotenv(source);
        case 'toml':
            return tokenizeToml(source);
        case 'yaml':
            return tokenizeYaml(source);
        case 'prompt':
            return tokenizePrompt(source);
        default:
            return source ? [{ kind: 'plain', text: source }] : [];
    }
}

export function joinTokens(tokens: Tok[]): string {
    return tokens.map((t) => t.text).join('');
}
