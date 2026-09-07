// 配置原文分词：给 CodeMirror 着色用。拼回去必须等于原文。

export type SyntaxMode = 'json' | 'dot_env' | 'toml' | 'plain';

export type TokKind = 'key' | 'string' | 'number' | 'bool' | 'null' | 'punct' | 'comment' | 'space' | 'plain';

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

export function tokenize(source: string, mode: SyntaxMode): Tok[] {
    switch (mode) {
        case 'json':
            return tokenizeJson(source);
        case 'dot_env':
            return tokenizeDotenv(source);
        case 'toml':
            return tokenizeToml(source);
        default:
            return source ? [{ kind: 'plain', text: source }] : [];
    }
}

export function joinTokens(tokens: Tok[]): string {
    return tokens.map((t) => t.text).join('');
}
