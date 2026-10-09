// CodeMirror 共用外观：SyntaxTextEditor（配置原文）和 JsonCodeEditor（调试台参数）
// 必须长得一样，所以主题和着色都收在这里，两边各自 import，不各抄一份。

import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { StateField } from '@codemirror/state';
import { joinTokens, tokenize, type SyntaxMode, type TokKind } from './syntaxTokens';

// `.ncd-syn-*` 这组 class 是着色装饰和主题之间的约定：装饰负责打标记，颜色只在下面的主题里定义。
export const editorTheme = EditorView.theme({
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
    // 提示词参数画成一枚小标签：一眼分得出哪些是麦麦要填进去的
    '.ncd-syn-param': {
        color: 'var(--color-brand)',
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 11%, transparent)',
        borderRadius: '4px',
        padding: '1px 2px',
    },
});

// 只有带语法诊断、补全的编辑器才需要：CodeMirror 自带的槽、气泡样式是浅色底，
// 放进深色主题里会突兀，所以统一改成设计令牌。SyntaxTextEditor 没有这些扩展，不引入。
export const editorPopupTheme = EditorView.theme({
    '.cm-gutters': {
        backgroundColor: 'var(--color-inset)',
        color: 'var(--color-text-tertiary)',
        border: 'none',
    },
    '.cm-lineNumbers .cm-gutterElement': {
        minWidth: '2.5em',
        padding: '0 6px 0 8px',
    },
    '.cm-foldGutter .cm-gutterElement': { padding: '0 4px', cursor: 'pointer' },
    '.cm-activeLine': {
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 5%, transparent)',
    },
    '.cm-activeLineGutter': {
        color: 'var(--color-text)',
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 9%, transparent)',
    },
    '.cm-foldPlaceholder': {
        color: 'var(--color-text-secondary)',
        backgroundColor: 'var(--color-elevated)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 'var(--radius-xs)',
        padding: '0 4px',
    },
    '.cm-lintRange-error': {
        backgroundImage: 'none',
        textDecoration: 'underline wavy var(--color-danger)',
    },
    '.cm-lintRange-warning': {
        backgroundImage: 'none',
        textDecoration: 'underline wavy var(--color-warning)',
    },
    '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': {
        backgroundColor: 'color-mix(in srgb, var(--color-brand) 22%, transparent)',
        outline: 'none',
    },
    '.cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket': {
        backgroundColor: 'color-mix(in srgb, var(--color-danger) 22%, transparent)',
    },
    '.cm-tooltip': {
        backgroundColor: 'var(--color-elevated)',
        color: 'var(--color-text)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 'var(--radius-sm)',
        boxShadow: 'var(--shadow-popover)',
        fontSize: '12px',
        // 不能在 .cm-tooltip 上 overflow:hidden：补全说明面板 .cm-completionInfo 是它的子元素，
        // 却定位在它左右外侧（left/right: 100%），一裁就整块看不见。圆角改由内部列表自己裁。
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': {
        fontFamily: 'var(--font-mono)',
        maxHeight: '14em',
        borderRadius: 'var(--radius-sm)',
    },
    // 诊断气泡没有外挂的子面板，可以放心裁圆角
    '.cm-tooltip.cm-tooltip-lint': { overflow: 'hidden' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li': { padding: '2px 8px' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
        backgroundColor: 'var(--color-brand-soft)',
        color: 'var(--color-text)',
    },
    '.cm-completionDetail': { color: 'var(--color-text-tertiary)', fontStyle: 'normal' },
    '.cm-completionMatchedText': {
        color: 'var(--color-brand)',
        textDecoration: 'none',
        fontWeight: '600',
    },
    '.cm-tooltip.cm-completionInfo': {
        padding: '6px 8px',
        maxWidth: '22em',
        whiteSpace: 'pre-wrap',
        color: 'var(--color-text-secondary)',
    },
    '.cm-diagnostic': { padding: '4px 8px', color: 'var(--color-text)' },
    '.cm-diagnostic-error': { borderLeft: '3px solid var(--color-danger)' },
    '.cm-diagnostic-warning': { borderLeft: '3px solid var(--color-warning)' },
});

const MARK: Partial<Record<TokKind, Decoration>> = {
    param: Decoration.mark({ class: 'ncd-syn-param' }),
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
    // 分词拼回去和原文对不上就宁可不着色，也不能把标记打偏
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

/** 按 syntaxTokens 的分词给文档着色，颜色见 editorTheme 里的 `.ncd-syn-*`。 */
export function syntaxColorField(mode: SyntaxMode) {
    return StateField.define<DecorationSet>({
        create: (state) => buildDecos(state.doc.toString(), mode),
        update: (deco, tr) => (tr.docChanged ? buildDecos(tr.state.doc.toString(), mode) : deco),
        provide: (field) => EditorView.decorations.from(field),
    });
}
