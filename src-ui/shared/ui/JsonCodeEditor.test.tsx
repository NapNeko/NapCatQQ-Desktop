import { act, fireEvent, render } from '@testing-library/react';
import { CompletionContext, type Completion, type CompletionResult } from '@codemirror/autocomplete';
import { Compartment, EditorSelection, EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { createRef } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { JsonCodeEditor, schemaKeyCompletion, type JsonCodeEditorHandle } from './JsonCodeEditor';

const SCHEMA = {
    type: 'object',
    properties: {
        group_id: { type: 'number', description: '群号' },
        message_type: { type: 'string', enum: ['private', 'group'] },
        auto_escape: { type: 'boolean' },
        flag: { anyOf: [{ const: 'a' }, { const: 'b' }] },
    },
    required: ['group_id'],
};

/** `|` 标出光标位置，返回补全结果和构造补全时用的状态。 */
async function complete(
    marked: string,
    opts: { schema?: Record<string, unknown> | null; explicit?: boolean } = {},
) {
    const pos = marked.indexOf('|');
    const doc = marked.replace('|', '');
    const state = EditorState.create({ doc, selection: EditorSelection.cursor(pos) });
    const source = schemaKeyCompletion(opts.schema === undefined ? SCHEMA : opts.schema);
    const result = (await source(new CompletionContext(state, pos, opts.explicit ?? false))) as CompletionResult | null;
    return { result, doc, pos };
}

const labels = (r: CompletionResult | null) => r?.options.map((o) => o.label) ?? [];

/** 真的把某个补全项应用到文档上，返回应用后的文本。 */
function applyOption(doc: string, pos: number, result: CompletionResult, label: string): string {
    const view = new EditorView({ doc, selection: EditorSelection.cursor(pos), parent: document.body });
    try {
        const option = result.options.find((o) => o.label === label) as Completion;
        (option.apply as (v: EditorView, c: Completion, from: number, to: number) => void)(view, option, result.from, pos);
        return view.state.doc.toString();
    } finally {
        view.destroy();
    }
}

describe('schemaKeyCompletion 键名', () => {
    it('引号里补顶层属性，带类型、必填和描述', async () => {
        const { result } = await complete('{"|');
        expect(labels(result).sort()).toEqual(['auto_escape', 'flag', 'group_id', 'message_type']);
        const groupId = result?.options.find((o) => o.label === 'group_id');
        expect(groupId).toMatchObject({ detail: 'number · 必填', info: '群号', boost: 1 });
        // 补全范围从开引号后面算起，敲下的前缀参与过滤
        expect(result?.from).toBe(2);
    });

    it('已经写过的键不再出现', async () => {
        const { result } = await complete('{"group_id": 1, "|');
        expect(labels(result)).not.toContain('group_id');
        expect(labels(result)).toContain('message_type');
    });

    it('引号外：敲了字或手动唤起才补，光标刚过 { 或逗号不打扰', async () => {
        expect((await complete('{ |')).result).toBeNull();
        expect((await complete('{"group_id": 1, |')).result).toBeNull();
        expect(labels((await complete('{ gro|')).result)).toContain('group_id');
        expect(labels((await complete('{ |', { explicit: true })).result)).toContain('group_id');
    });

    it('只补顶层：嵌套对象、数组里不给', async () => {
        expect((await complete('{"a": {"|')).result).toBeNull();
        expect((await complete('[{"|')).result).toBeNull();
        expect((await complete('{"a": [1, "|')).result).toBeNull();
    });

    it('没有 schema 或没有 properties 时不补', async () => {
        expect((await complete('{"|', { schema: null })).result).toBeNull();
        expect((await complete('{"|', { schema: { type: 'object' } })).result).toBeNull();
    });

    it('allOf / anyOf 分支里的属性也收进来，required 只认整体必填', async () => {
        const { result } = await complete('{"|', {
            schema: {
                allOf: [{ properties: { a: { type: 'string' } }, required: ['a'] }],
                anyOf: [{ properties: { b: { type: 'string' } }, required: ['b'] }],
            },
        });
        expect(result?.options.find((o) => o.label === 'a')).toMatchObject({ detail: 'string · 必填' });
        expect(result?.options.find((o) => o.label === 'b')).toMatchObject({ detail: 'string' });
    });

    it('应用补全：补上收尾引号和冒号', async () => {
        const typed = await complete('{"gr|');
        expect(applyOption(typed.doc, typed.pos, typed.result!, 'group_id')).toBe('{"group_id": ');

        // 冒号已经在后面就不再补；光标后残余的键名字符一并替换
        const mid = await complete('{"gr|oup": 1}');
        expect(applyOption(mid.doc, mid.pos, mid.result!, 'group_id')).toBe('{"group_id": 1}');

        const bare = await complete('{ gro|');
        expect(applyOption(bare.doc, bare.pos, bare.result!, 'group_id')).toBe('{ "group_id": ');
    });
});

describe('schemaKeyCompletion 值', () => {
    it('字符串枚举：引号里只补字符串，写成 JSON 字符串', async () => {
        const { result, doc, pos } = await complete('{"message_type": "|');
        expect(labels(result)).toEqual(['private', 'group']);
        expect(applyOption(doc, pos, result!, 'group')).toBe('{"message_type": "group"');
    });

    it('引号外补值：字符串加引号，布尔直接给 true / false，冒号后不用敲字就弹', async () => {
        const str = await complete('{"message_type": |');
        expect(labels(str.result)).toEqual(['private', 'group']);
        expect(applyOption(str.doc, str.pos, str.result!, 'private')).toBe('{"message_type": "private"');

        const bool = await complete('{"auto_escape": |');
        expect(labels(bool.result)).toEqual(['true', 'false']);
        expect(applyOption(bool.doc, bool.pos, bool.result!, 'true')).toBe('{"auto_escape": true');

        const partial = await complete('{"auto_escape": tr|');
        expect(applyOption(partial.doc, partial.pos, partial.result!, 'true')).toBe('{"auto_escape": true');
    });

    it('const 和 anyOf 里的 const 都算可选值', async () => {
        expect(labels((await complete('{"flag": "|')).result)).toEqual(['a', 'b']);
    });

    it('值已经写完（后面该跟逗号）时不再补；没有枚举的属性不补', async () => {
        expect((await complete('{"message_type": "private" |')).result).toBeNull();
        expect((await complete('{"auto_escape": true |')).result).toBeNull();
        expect((await complete('{"group_id": |')).result).toBeNull();
        expect((await complete('{"unknown": "|')).result).toBeNull();
    });
});

describe('JsonCodeEditor', () => {
    it('外部改 value 会同步进文档，编辑器里的输入回调 onChange', () => {
        const onChange = vi.fn();
        const { container, rerender } = render(<JsonCodeEditor value='{"a":1}' onChange={onChange} ariaLabel="参数" />);
        const content = container.querySelector('.cm-content') as HTMLElement;
        expect(content).toHaveAttribute('aria-label', '参数');
        expect(content.textContent).toContain('"a"');

        rerender(<JsonCodeEditor value='{"b":2}' onChange={onChange} ariaLabel="参数" />);
        expect(content.textContent).toContain('"b"');
        // 外部同步不应该被当成用户输入回调出去
        expect(onChange).not.toHaveBeenCalled();
    });

    it('Mod-Enter 触发 onSubmit；没传 onSubmit 时不吞按键', () => {
        const onSubmit = vi.fn();
        const { container, rerender } = render(<JsonCodeEditor value="{}" onChange={() => { }} onSubmit={onSubmit} />);
        const content = container.querySelector('.cm-content') as HTMLElement;
        fireEvent.keyDown(content, { key: 'Enter', ctrlKey: true });
        expect(onSubmit).toHaveBeenCalledTimes(1);

        rerender(<JsonCodeEditor value="{}" onChange={() => { }} />);
        fireEvent.keyDown(content, { key: 'Enter', ctrlKey: true });
        expect(onSubmit).toHaveBeenCalledTimes(1);
    });

    it('只在 readOnly 变化时重建视图；schema 变化热替换', () => {
        const { container, rerender } = render(<JsonCodeEditor value="{}" onChange={() => { }} />);
        const first = container.querySelector('.cm-editor');
        rerender(<JsonCodeEditor value="{}" onChange={() => { }} schema={SCHEMA} />);
        expect(container.querySelector('.cm-editor')).toBe(first);

        rerender(<JsonCodeEditor value="{}" onChange={() => { }} schema={SCHEMA} readOnly />);
        expect(container.querySelector('.cm-editor')).not.toBe(first);
        expect(container.querySelector('.cm-content')).toHaveAttribute('contenteditable', 'false');
    });

    it('只读时不挂诊断：截了一半的回包原文不该满屏报语法错', () => {
        const { container, rerender } = render(<JsonCodeEditor value='{"a":' onChange={() => { }} />);
        expect(container.querySelector('.cm-gutter-lint')).not.toBeNull();
        rerender(<JsonCodeEditor value='{"a":' onChange={() => { }} readOnly />);
        expect(container.querySelector('.cm-gutter-lint')).toBeNull();
    });

    it('schema 只在内容真的变了才重设补全配置：挂载、内容相同的新对象都不重设', () => {
        const reconfigure = vi.spyOn(Compartment.prototype, 'reconfigure');
        const noop = () => { };
        const { rerender } = render(<JsonCodeEditor value="{}" onChange={noop} schema={{ ...SCHEMA }} ariaLabel="参数" />);
        expect(reconfigure).not.toHaveBeenCalled();

        // 父组件每次渲染都会传一个内容相同的新对象
        rerender(<JsonCodeEditor value="{}" onChange={noop} schema={{ ...SCHEMA }} ariaLabel="参数" />);
        rerender(<JsonCodeEditor value="{}" onChange={noop} schema={JSON.parse(JSON.stringify(SCHEMA))} ariaLabel="参数" />);
        expect(reconfigure).not.toHaveBeenCalled();

        rerender(<JsonCodeEditor value="{}" onChange={noop} schema={{ properties: { other: { type: 'string' } } }} ariaLabel="参数" />);
        expect(reconfigure).toHaveBeenCalledTimes(1);

        rerender(<JsonCodeEditor value="{}" onChange={noop} schema={null} ariaLabel="参数" />);
        expect(reconfigure).toHaveBeenCalledTimes(2);

        rerender(<JsonCodeEditor value="{}" onChange={noop} schema={null} ariaLabel="别的名字" />);
        expect(reconfigure).toHaveBeenCalledTimes(3);
    });

    it('主题不能在 .cm-tooltip 上裁剪溢出：补全说明面板挂在它外侧，一裁就看不见', () => {
        render(<JsonCodeEditor value="{}" onChange={() => { }} />);
        const css = [...document.head.querySelectorAll('style')].map((s) => s.textContent ?? '').join('\n');
        const rules = css.split('}').map((r) => r.replace(/\s+/g, ' ').trim());
        // 选择器以 `.cm-tooltip` 结尾（不含 .cm-tooltip-autocomplete 之类变体）的规则里不许出现 overflow
        const bareTooltip = rules.filter((r) => /\.cm-tooltip \{/.test(r) || /\.cm-tooltip, /.test(r));
        expect(bareTooltip.length).toBeGreaterThan(0);
        expect(bareTooltip.some((r) => /overflow/.test(r))).toBe(false);
        // 诊断气泡没有外挂子面板，圆角仍由它自己裁
        expect(rules.some((r) => /\.cm-tooltip\.cm-tooltip-lint \{.*overflow: hidden/.test(r))).toBe(true);
    });

    it('revealLine 把光标移到该行行首，越界时夹在文档范围内', () => {
        const ref = createRef<JsonCodeEditorHandle>();
        const { container } = render(
            <JsonCodeEditor value={'{\n  "a": 1,\n  "b": 2\n}'} onChange={() => { }} handleRef={ref} />,
        );
        act(() => ref.current?.revealLine(3));
        const view = EditorView.findFromDOM(container.querySelector('.cm-editor') as HTMLElement);
        expect(view?.state.selection.main.head).toBe(view?.state.doc.line(3).from);

        act(() => ref.current?.revealLine(99));
        expect(view?.state.selection.main.head).toBe(view?.state.doc.line(4).from);
    });
});
