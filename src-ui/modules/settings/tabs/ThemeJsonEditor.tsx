// 基于主题 token 的 JSON 高亮编辑器：下层 pre 着色，上层透明 textarea 编辑。
// 不引入 shiki/prism，颜色全部走 text-brand / success / warning / info 等语义色。

import {
    useCallback,
    useMemo,
    useRef,
    type CSSProperties,
    type ReactNode,
} from 'react';
import { cn } from '../../../shared/utils/cn';

type TokKind =
    | 'key'
    | 'string'
    | 'number'
    | 'bool'
    | 'null'
    | 'punct'
    | 'space'
    | 'plain';

interface Tok {
    kind: TokKind;
    text: string;
}

const KIND_CLASS: Record<TokKind, string> = {
    key: 'text-brand',
    string: 'text-success',
    number: 'text-info',
    bool: 'text-warning',
    null: 'text-warning',
    punct: 'text-text-tertiary',
    space: 'text-text',
    plain: 'text-text',
};

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
            const isKey = source[k] === ':';
            push(isKey ? 'key' : 'string', lit);
            i = j;
            continue;
        }

        if (
            ch === '-' ||
            (ch >= '0' && ch <= '9')
        ) {
            let j = i + 1;
            while (j < n) {
                const c = source[j]!;
                if (
                    (c >= '0' && c <= '9') ||
                    c === '.' ||
                    c === 'e' ||
                    c === 'E' ||
                    c === '+' ||
                    c === '-'
                ) {
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

function renderTokens(tokens: Tok[]): ReactNode {
    return tokens.map((t, idx) => (
        <span key={idx} className={KIND_CLASS[t.kind]}>
            {t.text}
        </span>
    ));
}

interface Props {
    value: string;
    onChange: (next: string) => void;
    invalid?: boolean;
    'aria-label'?: string;
    className?: string;
}

export function ThemeJsonEditor({
    value,
    onChange,
    invalid = false,
    'aria-label': ariaLabel,
    className,
}: Props) {
    const preRef = useRef<HTMLPreElement | null>(null);
    const taRef = useRef<HTMLTextAreaElement | null>(null);
    const tokens = useMemo(() => tokenizeJson(value), [value]);

    const syncScroll = useCallback(() => {
        const ta = taRef.current;
        const pre = preRef.current;
        if (!ta || !pre) return;
        pre.scrollTop = ta.scrollTop;
        pre.scrollLeft = ta.scrollLeft;
    }, []);

    const shared: CSSProperties = {
        tabSize: 2,
    };

    return (
        <div
            className={cn(
                'relative min-h-0 w-full flex-1 overflow-hidden rounded-sm bg-field',
                'border outline-none transition-colors duration-150',
                invalid
                    ? 'border-danger focus-within:border-danger focus-within:ring-2 focus-within:ring-danger focus-within:ring-inset'
                    : 'border-border-subtle focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                className,
            )}
        >
            <pre
                ref={preRef}
                aria-hidden
                className={cn(
                    'pointer-events-none absolute inset-0 m-0 overflow-x-hidden overflow-y-auto',
                    // pre-wrap：保留缩进/换行，同时按容器宽度自动折行
                    'whitespace-pre-wrap break-words [overflow-wrap:anywhere] px-3 py-2',
                    'font-mono text-[12px] leading-relaxed',
                )}
                style={shared}
            >
                {value ? renderTokens(tokens) : <span className="text-text-tertiary">{'{}'}</span>}
                {/* 末尾换行占位，避免最后一行被裁 */}
                {'\n'}
            </pre>
            <textarea
                ref={taRef}
                aria-label={ariaLabel}
                value={value}
                spellCheck={false}
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                wrap="soft"
                onChange={(e) => onChange(e.target.value)}
                onScroll={syncScroll}
                className={cn(
                    'relative z-[1] block h-full min-h-[10rem] w-full resize-none overflow-x-hidden overflow-y-auto',
                    'bg-transparent px-3 py-2',
                    'font-mono text-[12px] leading-relaxed',
                    'whitespace-pre-wrap break-words [overflow-wrap:anywhere]',
                    'text-transparent caret-brand',
                    'outline-none border-0',
                    'selection:bg-brand/25 selection:text-transparent',
                )}
                style={{
                    ...shared,
                    // 部分 WebView 需要显式透明填充，否则会盖住下层着色
                    WebkitTextFillColor: 'transparent',
                    color: 'transparent',
                    caretColor: 'var(--brand-500)',
                }}
            />
        </div>
    );
}
