// 麦麦配置的界面 schema：bot.json / model.json 由 scripts/maibot/codegen.py 从上游 WebUI 的
// ConfigSchemaGenerator 导出（中文标签、说明、控件、范围、选项、高级标记、默认值）。
// 这里只放类型、按路径取节点、按路径不可变地读写值、照 schema 起新条目和查范围。

import botSchema from './bot.json';
import modelSchema from './model.json';

export type UiFieldType =
    'string' | 'integer' | 'number' | 'boolean' | 'select' | 'array' | 'object';

export interface UiField {
    name: string;
    type: UiFieldType;
    label?: string;
    description?: string;
    default?: unknown;
    options?: string[];
    minValue?: number;
    maxValue?: number;
    step?: number;
    'x-widget'?: string;
    'x-option-labels'?: Record<string, string>;
    'x-option-descriptions'?: Record<string, string>;
    advanced?: boolean;
    'x-row'?: string;
    placeholder?: string;
    items?: { type?: string };
    'x-textarea-rows'?: number;
    /** 上游 WebUI 也不出的字段（如按命令放行的规则，只给聊天命令写） */
    hidden?: boolean;
}

export interface UiNode {
    className?: string;
    uiLabel?: string;
    uiAdvanced?: boolean;
    uiOrder?: number;
    uiParent?: string;
    fields: UiField[];
    nested?: Record<string, UiNode>;
}

export const BOT_SCHEMA = botSchema as unknown as UiNode;
export const MODEL_SCHEMA = modelSchema as unknown as UiNode;

export function nodeAt(root: UiNode, path: readonly string[]): UiNode | undefined {
    let node: UiNode | undefined = root;
    for (const key of path) node = node?.nested?.[key];
    return node;
}

export function fieldOf(node: UiNode | undefined, name: string): UiField | undefined {
    return node?.fields.find((f) => f.name === name);
}

type Bag = Record<string, unknown>;

export function getIn(obj: unknown, path: readonly string[]): unknown {
    let cur: unknown = obj;
    for (const key of path) {
        if (cur === null || typeof cur !== 'object') return undefined;
        cur = (cur as Bag)[key];
    }
    return cur;
}

/** 不可变地改一处：沿途每层都换新对象，其余引用不动 */
export function setIn<T>(obj: T, path: readonly string[], value: unknown): T {
    if (path.length === 0) return value as T;
    const [head, ...rest] = path;
    const src = (obj ?? {}) as Bag;
    const copy: Bag = Array.isArray(src) ? ([...src] as unknown as Bag) : { ...src };
    copy[head] = setIn(src[head], rest, value);
    return copy as T;
}

function zeroOf(field: UiField): unknown {
    switch (field.type) {
        case 'boolean':
            return false;
        case 'integer':
        case 'number':
            return field.minValue ?? 0;
        case 'select':
            return field.options?.[0] ?? '';
        case 'array':
            return [];
        case 'object':
            return {};
        default:
            return '';
    }
}

const fmt = (v: number) => (Number.isInteger(v) ? v.toFixed(0) : String(v));

/**
 * 照 schema 查范围和可选值，措辞和路径同后端 IssueSink 的 range / one_of（后端那份也是从同一批
 * pydantic 约束生成的），保存前就能在字段上标红，不用等后端退回来。
 */
export function schemaIssues(
    node: UiNode,
    value: unknown,
    prefix: string,
): { path: string; message: string }[] {
    const out: { path: string; message: string }[] = [];
    const walk = (n: UiNode, v: unknown, path: string) => {
        if (v === null || typeof v !== 'object') return;
        const bag = v as Bag;
        for (const f of n.fields) {
            const here = `${path}/${f.name}`;
            const val = bag[f.name];
            const sub = n.nested?.[f.name];
            if (sub && Array.isArray(val)) {
                val.forEach((item, i) => walk(sub, item, `${here}/${i}`));
            } else if (sub) {
                walk(sub, val, here);
            } else if (typeof val === 'number') {
                const lo = f.minValue;
                const hi = f.maxValue;
                if (lo !== undefined && hi !== undefined && (val < lo || val > hi)) {
                    out.push({ path: here, message: `要在 ${fmt(lo)} 到 ${fmt(hi)} 之间` });
                } else if (lo !== undefined && hi === undefined && val < lo) {
                    out.push({ path: here, message: `不能小于 ${fmt(lo)}` });
                } else if (hi !== undefined && lo === undefined && val > hi) {
                    out.push({ path: here, message: `不能大于 ${fmt(hi)}` });
                }
            } else if (
                // 只认 Literal 出来的 select；普通字符串挂的 options 只是建议值，后端和上游都不拦
                f.type === 'select' &&
                typeof val === 'string' &&
                f.options?.length &&
                !f.options.includes(val)
            ) {
                out.push({
                    path: here,
                    message: `只能是 ${f.options.join(' / ')} 之一，现在是 ${JSON.stringify(val)}`,
                });
            }
        }
    };
    walk(node, value, prefix);
    return out;
}

/** 列表里「加一条」：有上游默认值用默认值，没有就按类型给个空的；嵌套的小节照样递归 */
export function newItemFor(node: UiNode): Bag {
    const out: Bag = {};
    for (const f of node.fields) {
        const sub = node.nested?.[f.name];
        // 默认 null 的是可选字段，不写就是没设
        if (f.default === null) continue;
        if (f.default !== undefined) out[f.name] = structuredClone(f.default);
        else if (sub && f.type === 'object') out[f.name] = newItemFor(sub);
        else out[f.name] = zeroOf(f);
    }
    return out;
}
