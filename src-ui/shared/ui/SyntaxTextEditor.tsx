// 配置原文编辑。着色和光标都由 CodeMirror 画，避免 pre+textarea 叠层在 WebView2 里错位。

import { useEffect, useRef } from 'react';
import {
    Decoration,
    EditorView,
    drawSelection,
    keymap,
    placeholder as cmPlaceholder,
    type DecorationSet,
} from '@codemirror/view';
import { Compartment, EditorState, StateField } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { cn } from '../utils/cn';
import { joinTokens, tokenize, type SyntaxMode, type TokKind } from './syntaxTokens';

export type { SyntaxMode };

const EMPTY: Record<SyntaxMode, string> = {
    json: '{}',
    dot_env: '# KEY=value',
    toml: '# table',
    plain: '',
};

const MARK: Partial<Record<TokKind, Decoration>> = {
    key: Decoration.mark({ class: 'ncd-syn-key' }),
    string: Decoration.mark({ class: 'ncd-syn-string' }),
    number: Decoration.mark({ class: 'ncd-syn-number' }),
    bool: Decoration.mark({ class: 'ncd-syn-bool' }),
    null: Decoration.mark({ class: 'ncd-syn-null' }),
    punct: Decoration.mark({ class: 'ncd-syn-punct' }),
    comment: Decoration.mark({ class: 'ncd-syn-comment' }),
};

function buildDecos(doc: string, mode: SyntaxMode): DecorationSet {
    const tokens = tokenize(doc, mode);
    if (joinTokens(tokens) !== doc) return Decoration.none;
    const ranges = [];
    let pos = 0;
    for (const t of tokens) {
        const from = pos;
        const to = pos + t.text.length;
        pos = to;
        const mark = MARK[t.kind];
        if (mark && from < to) ranges.push(mark.range(from, to));
    }
    return Decoration.set(ranges, true);
}

function tokenField(mode: SyntaxMode) {
    return StateField.define<DecorationSet>({
        create: (state) => buildDecos(state.doc.toString(), mode),
        update: (deco, tr) => (tr.docChanged ? buildDecos(tr.state.doc.toString(), mode) : deco),
        provide: (field) => EditorView.decorations.from(field),
    });
}

const editorTheme = EditorView.theme({
    '&': {
        height: '100%',
        overflow: 'hidden',
        backgroundColor: 'transparent',
        fontSize: '12px',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': {
        overflow: 'auto',
        fontFamily: 'var(--font-mono)',
        lineHeight: '1.6',
        fontFeatureSettings: '"liga" 0, "calt" 0',
    },
    '.cm-content': {
        color: 'var(--color-text)',
        caretColor: 'var(--color-brand)',
        padding: '10px 12px',
        minHeight: '100%',
    },
    '.cm-line': { padding: '0' },
    '.cm-cursor, .cm-cursor-primary': {
        borderLeftColor: 'var(--color-brand)',
    },
    '.cm-selectionBackground': {
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 25%, transparent)',
    },
    '&.cm-focused .cm-selectionBackground': {
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 25%, transparent)',
    },
    '.cm-placeholder': { color: 'var(--color-text-tertiary)' },
    '.ncd-syn-key': { color: 'var(--color-brand)' },
    '.ncd-syn-string': { color: 'var(--color-success)' },
    '.ncd-syn-number': { color: 'var(--color-info)' },
    '.ncd-syn-bool': { color: 'var(--color-warning)' },
    '.ncd-syn-null': { color: 'var(--color-warning)' },
    '.ncd-syn-punct': { color: 'var(--color-text-tertiary)' },
    '.ncd-syn-comment': { color: 'var(--color-text-tertiary)' },
});

export interface SyntaxTextEditorProps {
    value: string;
    onChange: (next: string) => void;
    mode?: SyntaxMode;
    invalid?: boolean;
    disabled?: boolean;
    /** 软折行（Webhook 模板）；配置原文默认关。 */
    wrap?: boolean;
    'aria-label'?: string;
    className?: string;
}

export function SyntaxTextEditor({
    value,
    onChange,
    mode = 'json',
    invalid = false,
    disabled = false,
    wrap = false,
    'aria-label': ariaLabel,
    className,
}: SyntaxTextEditorProps) {
    const hostRef = useRef<HTMLDivElement>(null);
    const viewRef = useRef<EditorView | null>(null);
    const gateRef = useRef(new Compartment());
    const onChangeRef = useRef(onChange);
    const valueRef = useRef(value);
    onChangeRef.current = onChange;
    valueRef.current = value;

    useEffect(() => {
        const host = hostRef.current;
        if (!host) return;

        const view = new EditorView({
            parent: host,
            state: EditorState.create({
                doc: valueRef.current,
                extensions: [
                    editorTheme,
                    drawSelection(),
                    history(),
                    keymap.of([...defaultKeymap, ...historyKeymap]),
                    EditorState.tabSize.of(2),
                    tokenField(mode),
                    cmPlaceholder(EMPTY[mode]),
                    wrap ? EditorView.lineWrapping : [],
                    EditorView.contentAttributes.of({
                        'aria-label': ariaLabel ?? '配置文件',
                        spellcheck: 'false',
                        autocorrect: 'off',
                        autocapitalize: 'off',
                    }),
                    gateRef.current.of([
                        EditorView.editable.of(!disabled),
                        EditorState.readOnly.of(!!disabled),
                    ]),
                    EditorView.updateListener.of((update) => {
                        if (!update.docChanged) return;
                        const next = update.state.doc.toString();
                        if (next !== valueRef.current) onChangeRef.current(next);
                    }),
                ],
            }),
        });
        viewRef.current = view;
        const ro = new ResizeObserver(() => view.requestMeasure());
        ro.observe(host);
        return () => {
            ro.disconnect();
            view.destroy();
            viewRef.current = null;
        };
        // value 只作初始文档；之后由下面的 effect 对齐。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mode, wrap, ariaLabel]);

    useEffect(() => {
        viewRef.current?.dispatch({
            effects: gateRef.current.reconfigure([
                EditorView.editable.of(!disabled),
                EditorState.readOnly.of(!!disabled),
            ]),
        });
    }, [disabled]);

    useEffect(() => {
        const view = viewRef.current;
        if (!view) return;
        const cur = view.state.doc.toString();
        if (cur === value) return;
        view.dispatch({
            changes: { from: 0, to: view.state.doc.length, insert: value },
        });
    }, [value]);

    return (
        <div
            className={cn(
                // h-0 flex-1：给 CodeMirror 一个确定高度。否则文档把 .cm-editor 撑开，
                // 外层 overflow-hidden 直接裁掉，滚动条出不来。
                'relative h-0 min-h-0 w-full flex-1 overflow-hidden rounded-sm bg-inset',
                'border outline-none transition-colors duration-150',
                invalid
                    ? 'border-danger focus-within:border-danger focus-within:ring-2 focus-within:ring-danger focus-within:ring-inset'
                    : 'border-border-subtle focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                disabled && 'opacity-60',
                className,
            )}
        >
            <div ref={hostRef} className="absolute inset-0 overflow-hidden" />
        </div>
    );
}
