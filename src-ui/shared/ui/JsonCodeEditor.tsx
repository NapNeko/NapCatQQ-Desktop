// JSON 编辑器：调试台的参数输入和响应查看共用。
// 在 SyntaxTextEditor 的基础上加了 JSON 语法诊断、括号配对，以及按 JSON Schema 补全顶层键名和枚举值。

import { useEffect, useImperativeHandle, useMemo, useRef, type Ref } from 'react';
import { EditorView, drawSelection, keymap } from '@codemirror/view';
import { Compartment, EditorState } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { json, jsonParseLinter } from '@codemirror/lang-json';
import { bracketMatching } from '@codemirror/language';
import { linter, lintGutter } from '@codemirror/lint';
import {
    autocompletion,
    type Completion,
    type CompletionContext,
    type CompletionResult,
    type CompletionSource,
} from '@codemirror/autocomplete';
import { cn } from '../utils/cn';
import { editorPopupTheme, editorTheme, syntaxColorField } from './codemirrorTheme';

export interface JsonCodeEditorHandle {
    focus: () => void;
    /** 光标跳到指定行（从 1 开始）并滚到视区中央，给「错误在第几行」这类跳转用。 */
    revealLine: (line: number) => void;
}

export interface JsonCodeEditorProps {
    value: string;
    onChange: (value: string) => void;
    /** 当前动作的参数 JSON Schema；传了才有补全。 */
    schema?: Record<string, unknown> | null;
    readOnly?: boolean;
    /** Mod-Enter 触发，一般是「发送」。 */
    onSubmit?: () => void;
    ariaLabel?: string;
    /** 宿主要有确定高度：flex 列里给 flex-1（默认已带），或直接给固定 h-*。 */
    className?: string;
    handleRef?: Ref<JsonCodeEditorHandle>;
}

// ---------------------------------------------------------------------------
// Schema 补全
// ---------------------------------------------------------------------------

type SchemaObject = Record<string, unknown>;

function isRecord(v: unknown): v is SchemaObject {
    return typeof v === 'object' && v !== null && !Array.isArray(v);
}

interface PropertyEntry {
    name: string;
    /** 同名属性可能散在 allOf / anyOf 的多个分支里，全部留着，取值和描述都从中合并。 */
    schemas: SchemaObject[];
    required: boolean;
}

const COMBINATOR_KEYS = ['allOf', 'anyOf', 'oneOf'] as const;
const MAX_SCHEMA_DEPTH = 4;

/** 顶层 properties，连同 allOf / anyOf / oneOf 各分支里的一起收齐。NapCat 的 payload 常把参数拆在分支里。 */
function collectProperties(schema: unknown): PropertyEntry[] {
    const map = new Map<string, PropertyEntry>();
    const visit = (node: unknown, depth: number, requiredCounts: boolean) => {
        if (!isRecord(node) || depth > MAX_SCHEMA_DEPTH) return;
        const requiredHere = new Set<string>(
            requiredCounts && Array.isArray(node.required)
                ? node.required.filter((n): n is string => typeof n === 'string')
                : [],
        );
        if (isRecord(node.properties)) {
            for (const [name, sub] of Object.entries(node.properties)) {
                if (!isRecord(sub)) continue;
                const entry = map.get(name) ?? { name, schemas: [], required: false };
                entry.schemas.push(sub);
                if (requiredHere.has(name)) entry.required = true;
                map.set(name, entry);
            }
        }
        // anyOf / oneOf 只是「其中一种」，里面的 required 不能算整体必填
        for (const key of COMBINATOR_KEYS) {
            const branches = node[key];
            if (!Array.isArray(branches)) continue;
            for (const branch of branches) visit(branch, depth + 1, requiredCounts && key === 'allOf');
        }
    };
    visit(schema, 0, true);
    return [...map.values()];
}

function propertyDescription(entry: PropertyEntry): string | undefined {
    for (const s of entry.schemas) {
        if (typeof s.description === 'string' && s.description.trim()) return s.description.trim();
    }
    return undefined;
}

function propertyTypeLabel(entry: PropertyEntry): string {
    for (const s of entry.schemas) {
        const t = s.type;
        if (typeof t === 'string') return t;
        if (Array.isArray(t) && t.length > 0) return t.filter((x) => typeof x === 'string').join('|');
        if (Array.isArray(s.enum) || 'const' in s) return 'enum';
    }
    return '';
}

/** 枚举、const 和布尔的可选值，按出现顺序去重。 */
function collectValueChoices(entry: PropertyEntry): unknown[] {
    const out: unknown[] = [];
    const seen = new Set<string>();
    const add = (v: unknown) => {
        const k = JSON.stringify(v);
        if (k === undefined || seen.has(k)) return;
        seen.add(k);
        out.push(v);
    };
    const visit = (node: unknown, depth: number) => {
        if (!isRecord(node) || depth > MAX_SCHEMA_DEPTH) return;
        if ('const' in node) add(node.const);
        if (Array.isArray(node.enum)) node.enum.forEach(add);
        const types = Array.isArray(node.type) ? node.type : [node.type];
        if (types.includes('boolean')) {
            add(true);
            add(false);
        }
        for (const key of ['anyOf', 'oneOf'] as const) {
            const branches = node[key];
            if (Array.isArray(branches)) for (const b of branches) visit(b, depth + 1);
        }
    };
    for (const s of entry.schemas) visit(s, 0);
    return out;
}

// ---------------------------------------------------------------------------
// 光标处的 JSON 语境
// ---------------------------------------------------------------------------

interface Frame {
    kind: 'object' | 'array';
    /** 对象里正等一个键（刚 `{` 或刚 `,`）。 */
    expectKey: boolean;
    /** 已读到冒号、正在写值的那个键。 */
    key: string | null;
    /** 冒号前刚读完的键，等冒号确认。 */
    pendingKey: string | null;
    /** 当前值已经写完（后面只该跟逗号或收尾）。 */
    valueDone: boolean;
}

interface JsonCursorContext {
    stack: Frame[];
    inString: boolean;
    /** 光标所在字符串是不是键。 */
    stringIsKey: boolean;
    /** 开引号的位置。 */
    stringStart: number;
    /** 顶层对象里光标前已经写完的键，补全时排除。 */
    seenTopKeys: Set<string>;
}

function decodeJsonString(raw: string): string {
    try {
        return JSON.parse(`"${raw}"`) as string;
    } catch {
        return raw;
    }
}

/**
 * 从头扫到光标，得出光标落在哪种位置。
 * 不用语法树：输入到一半（缺引号、缺括号）时树会带错误节点，
 * 而补全恰恰发生在这种时候，手写扫描更稳。
 */
function scanJsonContext(text: string): JsonCursorContext {
    const stack: Frame[] = [];
    const seenTopKeys = new Set<string>();
    let inString = false;
    let stringIsKey = false;
    let stringStart = -1;
    let buf = '';
    let escaped = false;

    const top = () => stack[stack.length - 1];
    const endValueInParent = () => {
        const f = top();
        if (f && f.kind === 'object' && !f.expectKey) f.valueDone = true;
    };

    for (let i = 0; i < text.length; i += 1) {
        const ch = text[i]!;
        if (inString) {
            if (escaped) {
                escaped = false;
                buf += ch;
            } else if (ch === '\\') {
                escaped = true;
                buf += ch;
            } else if (ch === '"') {
                inString = false;
                const f = top();
                if (f && f.kind === 'object') {
                    if (stringIsKey) {
                        f.pendingKey = decodeJsonString(buf);
                        if (stack.length === 1) seenTopKeys.add(f.pendingKey);
                    } else {
                        f.valueDone = true;
                    }
                }
            } else {
                buf += ch;
            }
            continue;
        }
        switch (ch) {
            case '"': {
                const f = top();
                inString = true;
                stringStart = i;
                buf = '';
                escaped = false;
                stringIsKey = !!f && f.kind === 'object' && f.expectKey;
                break;
            }
            case '{':
            case '[':
                stack.push({
                    kind: ch === '{' ? 'object' : 'array',
                    expectKey: ch === '{',
                    key: null,
                    pendingKey: null,
                    valueDone: false,
                });
                break;
            case '}':
            case ']':
                stack.pop();
                endValueInParent();
                break;
            case ':': {
                const f = top();
                if (f && f.kind === 'object' && f.expectKey && f.pendingKey !== null) {
                    f.expectKey = false;
                    f.key = f.pendingKey;
                    f.valueDone = false;
                }
                break;
            }
            case ',': {
                const f = top();
                if (f && f.kind === 'object') {
                    f.expectKey = true;
                    f.key = null;
                    f.pendingKey = null;
                    f.valueDone = false;
                }
                break;
            }
            case ' ':
            case '\t':
            case '\n':
            case '\r':
                break;
            default:
                // 数字、true / false / null 之类裸值：值位置上出现了就算写过
                endValueInParent();
        }
    }
    return { stack, inString, stringIsKey, stringStart, seenTopKeys };
}

// ---------------------------------------------------------------------------
// 补全项的写入
// ---------------------------------------------------------------------------

/** 光标后紧跟的「键名残余 + 收尾引号」，写入补全时一并吃掉，避免出现 `""`。 */
function stringTailLength(view: EditorView, at: number): number {
    const tail = view.state.doc.sliceString(at, Math.min(view.state.doc.length, at + 200));
    return /^[^"\n]*"?/.exec(tail)?.[0].length ?? 0;
}

function applyKey(name: string, inString: boolean): NonNullable<Completion['apply']> {
    return (view, _completion, from, to) => {
        const end = inString ? to + stringTailLength(view, to) : to;
        let insert = inString ? `${JSON.stringify(name).slice(1, -1)}"` : JSON.stringify(name);
        // 冒号还没写就替用户补上，写完直接进入值
        const after = view.state.doc.sliceString(end, Math.min(view.state.doc.length, end + 32));
        if (!/^\s*:/.test(after)) insert += ': ';
        view.dispatch({
            changes: { from, to: end, insert },
            selection: { anchor: from + insert.length },
            userEvent: 'input.complete',
        });
    };
}

function applyValue(value: unknown, inString: boolean): NonNullable<Completion['apply']> {
    return (view, _completion, from, to) => {
        const end = inString ? to + stringTailLength(view, to) : to;
        const insert = inString ? `${JSON.stringify(String(value)).slice(1, -1)}"` : JSON.stringify(value);
        view.dispatch({
            changes: { from, to: end, insert },
            selection: { anchor: from + insert.length },
            userEvent: 'input.complete',
        });
    };
}

/**
 * 按参数 JSON Schema 补全：
 * - 顶层对象里写键名时，列出还没写过的属性，描述作为说明；
 * - 键后面的冒号之后，列出该属性的 enum / const（布尔给 true / false）。
 * 只做顶层：更深的层级 Schema 形态太杂，补错了比不补更烦。
 */
export function schemaKeyCompletion(schema: Record<string, unknown> | null | undefined): CompletionSource {
    const properties = collectProperties(schema);
    const byName = new Map(properties.map((p) => [p.name, p]));

    return (context: CompletionContext): CompletionResult | null => {
        if (properties.length === 0) return null;
        const { state, pos } = context;
        const prefix = state.doc.sliceString(0, pos);
        const scan = scanJsonContext(prefix);

        let from: number;
        let inString = false;
        let bareWord = false;
        let ctx = scan;
        if (scan.inString) {
            inString = true;
            from = scan.stringStart + 1;
        } else {
            // 裸词（没加引号的键、true / fal…）：以词首为起点，并按词之前的状态判断位置
            const word = /[\w$.+-]*$/.exec(prefix)?.[0] ?? '';
            bareWord = word.length > 0;
            from = pos - word.length;
            ctx = bareWord ? scanJsonContext(prefix.slice(0, from)) : scan;
        }

        // 只补顶层对象：栈里正好一层，且是对象
        if (ctx.stack.length !== 1) return null;
        const frame = ctx.stack[0]!;
        if (frame.kind !== 'object') return null;

        const keyPosition = inString ? ctx.stringIsKey : frame.expectKey;
        if (keyPosition) {
            // 引号外还没敲字就弹键名列表太吵（`{` 后、`,` 后都会触发），手动唤起或敲了字再给
            if (!inString && !bareWord && !context.explicit) return null;
            const options: Completion[] = properties
                .filter((p) => !ctx.seenTopKeys.has(p.name))
                .map((p) => {
                    const typeLabel = propertyTypeLabel(p);
                    return {
                        label: p.name,
                        type: 'property',
                        detail: [typeLabel, p.required ? '必填' : ''].filter(Boolean).join(' · '),
                        info: propertyDescription(p),
                        boost: p.required ? 1 : 0,
                        apply: applyKey(p.name, inString),
                    };
                });
            if (options.length === 0) return null;
            return { from, options, validFor: inString ? /^[^"\\\n]*$/ : /^[\w$.-]*$/ };
        }

        // 值位置：得有个读到冒号的键，且值还没写完
        if (frame.key === null || (!inString && frame.valueDone)) return null;
        const entry = byName.get(frame.key);
        if (!entry) return null;
        // 落在字符串里只能补字符串值；数字、布尔要在引号外补
        const choices = collectValueChoices(entry).filter((v) => !inString || typeof v === 'string');
        if (choices.length === 0) return null;
        return {
            from,
            options: choices.map((v) => ({
                label: typeof v === 'string' ? v : JSON.stringify(v),
                type: 'enum',
                apply: applyValue(v, inString),
            })),
            validFor: inString ? /^[^"\\\n]*$/ : /^[\w$.+-]*$/,
        };
    };
}

// ---------------------------------------------------------------------------
// 组件
// ---------------------------------------------------------------------------

const parseLinter = jsonParseLinter();

// 空白文档不算错：参数区没填东西时不该红一片
function lintJson(view: EditorView) {
    return /\S/.test(view.state.doc.toString()) ? parseLinter(view) : [];
}

function contentAttributes(ariaLabel: string | undefined) {
    return EditorView.contentAttributes.of({
        'aria-label': ariaLabel ?? 'JSON 编辑器',
        spellcheck: 'false',
        autocorrect: 'off',
        autocapitalize: 'off',
    });
}

// 父组件常常每次渲染都传一个内容相同的新 schema 对象。按引用比较会让补全配置每次渲染都被重设，
// 敲字过程中把已经弹出的补全面板关掉，所以按内容算一个稳定的键来比。
let unkeyedSchemaCount = 0;
function schemaKeyOf(schema: Record<string, unknown> | null | undefined): string {
    if (!schema) return '';
    try {
        return JSON.stringify(schema);
    } catch {
        // 不可序列化（循环引用）时退回「每个对象各算一份」，行为等同按引用比较
        unkeyedSchemaCount += 1;
        return `#unkeyed:${unkeyedSchemaCount}`;
    }
}

function schemaExtension(schema: Record<string, unknown> | null | undefined) {
    return autocompletion({ override: [schemaKeyCompletion(schema)], icons: false });
}

export function JsonCodeEditor({
    value,
    onChange,
    schema,
    readOnly = false,
    onSubmit,
    ariaLabel,
    className,
    handleRef,
}: JsonCodeEditorProps) {
    const hostRef = useRef<HTMLDivElement>(null);
    const viewRef = useRef<EditorView | null>(null);
    const schemaGateRef = useRef(new Compartment());
    const attrsGateRef = useRef(new Compartment());
    const onChangeRef = useRef(onChange);
    const onSubmitRef = useRef(onSubmit);
    const valueRef = useRef(value);
    const schemaRef = useRef(schema);
    const ariaLabelRef = useRef(ariaLabel);
    // 每个视图创建时已经带上的 schema / aria-label，热替换时和它比，相同就不重设
    const appliedSchemaKeyRef = useRef<string | null>(null);
    const appliedAriaLabelRef = useRef<string | undefined>(undefined);
    const schemaKey = useMemo(() => schemaKeyOf(schema), [schema]);
    const schemaKeyRef = useRef(schemaKey);
    onChangeRef.current = onChange;
    onSubmitRef.current = onSubmit;
    valueRef.current = value;
    schemaRef.current = schema;
    ariaLabelRef.current = ariaLabel;
    schemaKeyRef.current = schemaKey;

    // 只有 readOnly 变化才重建视图；schema、aria-label 走 Compartment 热替换，
    // 否则每次切动作都会丢光标和撤销历史。
    useEffect(() => {
        const host = hostRef.current;
        if (!host) return;

        const view = new EditorView({
            parent: host,
            state: EditorState.create({
                doc: valueRef.current,
                extensions: [
                    editorTheme,
                    editorPopupTheme,
                    json(),
                    syntaxColorField('json'),
                    drawSelection(),
                    history(),
                    bracketMatching(),
                    // 只读是拿来看的（回包原文）：诊断是给编辑用的，超大回包只截了开头一段时满屏报错只会误导
                    ...(readOnly ? [] : [linter(lintJson, { delay: 300 }), lintGutter()]),
                    schemaGateRef.current.of(schemaExtension(schemaRef.current)),
                    // Mod-Enter 必须排在 defaultKeymap 前面，否则先被「插入空行」吃掉
                    keymap.of([
                        {
                            key: 'Mod-Enter',
                            run: () => {
                                const submit = onSubmitRef.current;
                                if (!submit) return false;
                                submit();
                                return true;
                            },
                        },
                        ...defaultKeymap,
                        ...historyKeymap,
                    ]),
                    EditorState.tabSize.of(2),
                    attrsGateRef.current.of(contentAttributes(ariaLabelRef.current)),
                    EditorView.editable.of(!readOnly),
                    EditorState.readOnly.of(readOnly),
                    EditorView.updateListener.of((update) => {
                        if (!update.docChanged) return;
                        const next = update.state.doc.toString();
                        if (next !== valueRef.current) onChangeRef.current(next);
                    }),
                ],
            }),
        });
        viewRef.current = view;
        appliedSchemaKeyRef.current = schemaKeyRef.current;
        appliedAriaLabelRef.current = ariaLabelRef.current;
        const ro = new ResizeObserver(() => view.requestMeasure());
        ro.observe(host);
        return () => {
            ro.disconnect();
            view.destroy();
            viewRef.current = null;
        };
    }, [readOnly]);

    // 挂载时视图已按当前值创建，这里只在内容真的变了才重设
    useEffect(() => {
        const view = viewRef.current;
        if (!view || appliedSchemaKeyRef.current === schemaKey) return;
        appliedSchemaKeyRef.current = schemaKey;
        view.dispatch({ effects: schemaGateRef.current.reconfigure(schemaExtension(schemaRef.current)) });
    }, [schemaKey]);

    useEffect(() => {
        const view = viewRef.current;
        if (!view || appliedAriaLabelRef.current === ariaLabel) return;
        appliedAriaLabelRef.current = ariaLabel;
        view.dispatch({ effects: attrsGateRef.current.reconfigure(contentAttributes(ariaLabel)) });
    }, [ariaLabel]);

    useImperativeHandle(
        handleRef,
        () => ({
            focus: () => viewRef.current?.focus(),
            revealLine: (line: number) => {
                const view = viewRef.current;
                if (!view) return;
                const n = Math.min(Math.max(1, Math.floor(line) || 1), view.state.doc.lines);
                const at = view.state.doc.line(n).from;
                view.dispatch({
                    selection: { anchor: at },
                    effects: EditorView.scrollIntoView(at, { y: 'center' }),
                });
                view.focus();
            },
        }),
        [],
    );

    // 外部改了 value（切动作、套用历史）才整篇替换；选区夹在新文档长度内，
    // 不然 CodeMirror 会把光标甩到末尾。用户自己敲出来的变化 cur === value，直接跳过。
    useEffect(() => {
        const view = viewRef.current;
        if (!view) return;
        if (view.state.doc.toString() === value) return;
        const { anchor, head } = view.state.selection.main;
        view.dispatch({
            changes: { from: 0, to: view.state.doc.length, insert: value },
            selection: { anchor: Math.min(anchor, value.length), head: Math.min(head, value.length) },
        });
    }, [value]);

    return (
        <div
            className={cn(
                // h-0 flex-1：给 CodeMirror 一个确定高度。否则文档把 .cm-editor 撑开，
                // 外层 overflow-hidden 直接裁掉，滚动条出不来。
                'relative h-0 min-h-0 w-full flex-1 overflow-hidden rounded-sm bg-inset',
                'border border-border-subtle outline-none transition-colors duration-150',
                'focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                className,
            )}
        >
            <div ref={hostRef} className="absolute inset-0 overflow-hidden" />
        </div>
    );
}
