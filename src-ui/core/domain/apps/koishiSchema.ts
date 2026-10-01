// Koishi 插件表单的 schema：Schemastery 序列化出来的 `{uid, refs}`（控制台画表单用的就是这份）。
// 这里只做解析和取值规则，渲染在 modules/apps/detail/koishi/SchemasteryForm.tsx。
//
// 取值规则跟上游控制台一致：配置里没写的键按 schema 默认值显示，保存时也不补默认值
// （上游写盘前会 simplify 掉和默认一样的值，写了反而多出一堆键）。

export interface SchemaMeta {
    default?: unknown;
    required?: boolean;
    description?: unknown;
    role?: string;
    min?: number;
    max?: number;
    step?: number;
    hidden?: unknown;
    collapse?: boolean;
    disabled?: unknown;
    extra?: Record<string, unknown>;
    pattern?: { source: string; flags?: string };
    badges?: { text: string; type?: string }[];
}

interface SchemaRef {
    type: string;
    meta?: SchemaMeta;
    dict?: Record<string, number>;
    list?: number[];
    inner?: number;
    sKey?: number;
    value?: unknown;
    bits?: Record<string, number>;
}

export interface SchemaJson {
    uid: number;
    refs: Record<string, SchemaRef>;
}

export interface SNode {
    uid: number;
    type: string;
    meta: SchemaMeta;
    dict?: Record<string, SNode>;
    list?: SNode[];
    inner?: SNode;
    sKey?: SNode;
    value?: unknown;
    bits?: Record<string, number>;
}

function isSchemaJson(v: unknown): v is SchemaJson {
    return !!v && typeof v === 'object' && 'uid' in v && 'refs' in v && typeof (v as SchemaJson).refs === 'object';
}

/** 把 refs 连成一张图（可能有环：递归的 schema 只是指回同一个 uid）；认不出返回 null */
export function hydrateSchema(json: unknown): SNode | null {
    if (!isSchemaJson(json)) return null;
    const nodes = new Map<string, SNode>();
    for (const [uid, ref] of Object.entries(json.refs)) {
        if (!ref || typeof ref.type !== 'string') continue;
        nodes.set(uid, { uid: Number(uid), type: ref.type, meta: ref.meta ?? {}, value: ref.value, bits: ref.bits });
    }
    const get = (uid: number | undefined) => (uid === undefined ? undefined : nodes.get(String(uid)));
    for (const [uid, ref] of Object.entries(json.refs)) {
        const node = nodes.get(uid);
        if (!node) continue;
        if (ref.dict) {
            node.dict = {};
            for (const [k, id] of Object.entries(ref.dict)) {
                const child = get(id);
                if (child) node.dict[k] = child;
            }
        }
        if (ref.list) node.list = ref.list.map(get).filter((n): n is SNode => !!n);
        node.inner = get(ref.inner);
        node.sKey = get(ref.sKey);
    }
    return get(json.uid) ?? null;
}

/** 描述可能是字符串，也可能是按语言分的表（`{ 'zh-CN': …, '': … }`） */
export function schemaText(v: unknown): string {
    if (typeof v === 'string') return v;
    if (!v || typeof v !== 'object') return '';
    const m = v as Record<string, unknown>;
    for (const k of ['zh-CN', 'zh', '', 'en-US', 'en']) {
        if (typeof m[k] === 'string') return m[k] as string;
    }
    const first = Object.values(m).find((x) => typeof x === 'string');
    return typeof first === 'string' ? first : '';
}

export function describe(node: SNode): string {
    return schemaText(node.meta.description);
}

/** 只认字面量 true：上游允许传函数（按值动态隐藏），序列化后就没了 */
export function isHidden(node: SNode): boolean {
    return node.meta.hidden === true;
}

export function isPrimitive(node: SNode): boolean {
    return ['string', 'number', 'natural', 'percent', 'boolean', 'const', 'date'].includes(node.type);
}

/** 联合里能选的分支：隐藏的（Koishi 的 computed 把 `$switch` 分支藏起来）去掉 */
export function visibleBranches(node: SNode): SNode[] {
    return (node.list ?? []).filter((b) => !isHidden(b) && b.type !== 'never');
}

/** 对象 / 交叉 / 带默认值包装之后的实际字段表（交叉里的对象按顺序拼起来） */
export function objectFields(node: SNode): { key: string; node: SNode }[] {
    if (node.type === 'object') return Object.entries(node.dict ?? {}).map(([key, n]) => ({ key, node: n }));
    if (node.type === 'intersect') return (node.list ?? []).flatMap(objectFields);
    return [];
}

/** 交叉里带标题的一段段（上游控制台按段画分组）；没有交叉就是一整段 */
export function objectSections(node: SNode): { title: string; fields: { key: string; node: SNode }[] }[] {
    if (node.type === 'intersect') {
        return (node.list ?? []).flatMap((part) => {
            if (part.type === 'union') {
                // 按判别字段选分支的那种，交给渲染器整段处理
                return [{ title: describe(part), fields: [{ key: '', node: part }] }];
            }
            return objectSections(part);
        });
    }
    if (node.type === 'object') return [{ title: describe(node), fields: objectFields(node) }];
    return [];
}

export function isObjectLike(node: SNode): boolean {
    return node.type === 'object' || node.type === 'intersect';
}

/** 联合的形状：全是常量 → 下拉；都是对象、有同一个常量键 → 按那个键分支；其余 → 取能画的第一个 */
export type UnionShape =
    | { kind: 'enum'; options: { value: unknown; label: string }[] }
    | { kind: 'tagged'; key: string; branches: { value: unknown; label: string; node: SNode }[] }
    | { kind: 'mixed'; branches: SNode[] };

function constKeyOf(node: SNode): Record<string, SNode> {
    const out: Record<string, SNode> = {};
    for (const f of objectFields(node)) if (f.node.type === 'const') out[f.key] = f.node;
    return out;
}

export function unionShape(node: SNode): UnionShape {
    const branches = visibleBranches(node);
    if (branches.length > 0 && branches.every((b) => b.type === 'const')) {
        return {
            kind: 'enum',
            options: branches.map((b) => ({ value: b.value, label: describe(b) || String(b.value) })),
        };
    }
    if (branches.length > 1 && branches.every(isObjectLike)) {
        const first = constKeyOf(branches[0]);
        const key = Object.keys(first).find((k) => branches.every((b) => k in constKeyOf(b)));
        if (key) {
            return {
                kind: 'tagged',
                key,
                branches: branches.map((b) => {
                    const c = constKeyOf(b)[key];
                    return { value: c.value, label: describe(c) || String(c.value), node: b };
                }),
            };
        }
    }
    return { kind: 'mixed', branches };
}

/** 标签联合里现在是哪一支：值里写了判别键就按它，没写就是第一支（上游 required(false) 的那支当默认） */
export function taggedBranch(
    shape: Extract<UnionShape, { kind: 'tagged' }>,
    value: Record<string, unknown> | undefined,
): number {
    const tag = value?.[shape.key];
    const hit = shape.branches.findIndex((b) => b.value === tag);
    if (hit >= 0) return hit;
    const optional = shape.branches.findIndex((b) => {
        const c = constKeyOf(b.node)[shape.key];
        return c && c.meta.required === false;
    });
    return optional >= 0 ? optional : 0;
}

/** 混合联合里按值的类型挑分支（`string | number` 这种），挑不出就第一支 */
export function mixedBranch(branches: SNode[], value: unknown): number {
    const kind = Array.isArray(value) ? 'array' : typeof value;
    const matches = (b: SNode) => {
        switch (b.type) {
            case 'string':
                return kind === 'string';
            case 'number':
            case 'natural':
            case 'percent':
                return kind === 'number';
            case 'boolean':
                return kind === 'boolean';
            case 'array':
            case 'tuple':
                return kind === 'array';
            case 'object':
            case 'intersect':
            case 'dict':
                return kind === 'object' && value !== null;
            case 'const':
                return b.value === value;
            default:
                return false;
        }
    };
    const hit = branches.findIndex(matches);
    return hit >= 0 ? hit : 0;
}

/** 显示用的值：配置里没写就用 schema 默认值 */
export function effectiveValue(node: SNode, value: unknown): unknown {
    return value === undefined ? node.meta.default : value;
}

/** 表单能不能画这个节点；画不了的交给 JSON 原文框 */
export function renderable(node: SNode, depth = 0): boolean {
    if (depth > 12) return false;
    switch (node.type) {
        case 'string':
        case 'number':
        case 'natural':
        case 'percent':
        case 'boolean':
        case 'const':
        case 'date':
        case 'any':
            return true;
        case 'object':
        case 'intersect':
            return objectFields(node).every((f) => isHidden(f.node) || renderable(f.node, depth + 1));
        case 'array':
        case 'dict':
            return !!node.inner && renderable(node.inner, depth + 1);
        case 'tuple':
            return (node.list ?? []).every((n) => renderable(n, depth + 1));
        case 'union':
            return visibleBranches(node).some((n) => renderable(n, depth + 1));
        case 'transform':
            return !!node.inner && renderable(node.inner, depth + 1);
        default:
            return false;
    }
}

/** 新加一条数组 / 字典项时给的初值 */
export function blankOf(node: SNode | undefined): unknown {
    if (!node) return '';
    if (node.meta.default !== undefined) return structuredClone(node.meta.default);
    switch (node.type) {
        case 'number':
        case 'natural':
        case 'percent':
            return node.meta.min ?? 0;
        case 'boolean':
            return false;
        case 'object':
        case 'intersect':
        case 'dict':
            return {};
        case 'array':
        case 'tuple':
            return [];
        case 'const':
            return node.value;
        case 'union': {
            const b = visibleBranches(node)[0];
            return blankOf(b);
        }
        default:
            return '';
    }
}

type Obj = Record<string, unknown>;

/** 顶层字段 + 按当前判别值展开的标签联合段里的字段（和表单看到的是同一批） */
export function visibleFields(node: SNode, value: Obj): { key: string; node: SNode }[] {
    if (!isObjectLike(node)) return [];
    const out: { key: string; node: SNode }[] = [];
    for (const s of objectSections(node)) {
        for (const f of s.fields) {
            if (f.key === '') {
                const shape = unionShape(f.node);
                const branchNode =
                    shape.kind === 'tagged'
                        ? shape.branches[taggedBranch(shape, value)]?.node
                        : shape.kind === 'mixed'
                          ? shape.branches.find(isObjectLike)
                          : undefined;
                if (branchNode) out.push(...visibleFields(branchNode, value));
                continue;
            }
            if (!isHidden(f.node)) out.push(f);
        }
    }
    return out;
}

/** 没写的键按 schema 默认值补上（深拷贝）：「原文 JSON」模式的起点，打开不是一张白纸 */
export function materializeConfig(node: SNode, value: Obj): Obj {
    const fields = visibleFields(node, value);
    const out: Obj = { ...value };
    for (const f of fields) {
        if (out[f.key] === undefined && f.node.meta.default !== undefined) {
            out[f.key] = structuredClone(f.node.meta.default);
        }
    }
    return out;
}

function deepEqual(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (Array.isArray(a) && Array.isArray(b)) {
        return a.length === b.length && a.every((v, i) => deepEqual(v, b[i]));
    }
    if (a && b && typeof a === 'object' && typeof b === 'object') {
        const ka = Object.keys(a as Obj);
        const kb = Object.keys(b as Obj);
        return (
            ka.length === kb.length && ka.every((k) => deepEqual((a as Obj)[k], (b as Obj)[k]))
        );
    }
    return false;
}

/** 写回前把和默认值深度相等的键删掉（上游 simplify 的语义：和默认一样的不落盘） */
export function simplifyConfig(node: SNode, value: Obj): Obj {
    const out: Obj = { ...value };
    for (const f of visibleFields(node, value)) {
        if (f.key in out && f.node.meta.default !== undefined && deepEqual(out[f.key], f.node.meta.default)) {
            delete out[f.key];
        }
    }
    return out;
}

/** 给原文 JSON 编辑器的补全用：Schemastery → 浅层 JSON Schema（顶层键、枚举、描述、默认值） */
export function toJsonSchema(node: SNode, value: Obj, depth = 0): Record<string, unknown> | null {
    if (depth > 2) return null;
    if (!isObjectLike(node)) return null;
    const properties: Record<string, unknown> = {};
    const required: string[] = [];
    for (const f of visibleFields(node, value)) {
        const sub = fieldJsonSchema(f.node, depth + 1);
        if (sub) properties[f.key] = sub;
        if (f.node.meta.required) required.push(f.key);
    }
    return { type: 'object', properties, ...(required.length ? { required } : {}) };
}

function fieldJsonSchema(node: SNode, depth: number): Record<string, unknown> | null {
    if (depth > 2) return null;
    const base: Record<string, unknown> = {};
    const desc = describe(node);
    if (desc) base.description = desc;
    if (node.meta.default !== undefined) base.default = node.meta.default;
    switch (node.type) {
        case 'string':
            base.type = 'string';
            break;
        case 'number':
        case 'natural':
        case 'percent':
            base.type = 'number';
            break;
        case 'boolean':
            base.type = 'boolean';
            break;
        case 'date':
            base.type = 'string';
            break;
        case 'const':
            base.const = node.value;
            break;
        case 'object':
        case 'intersect':
        case 'dict':
            base.type = 'object';
            break;
        case 'array':
        case 'tuple':
            base.type = 'array';
            break;
        case 'union': {
            const shape = unionShape(node);
            if (shape.kind === 'enum') base.enum = shape.options.map((o) => o.value);
            else base.type = 'object';
            break;
        }
        case 'any':
            break;
        default:
            return Object.keys(base).length > 0 ? base : null;
    }
    return base;
}

/** 必填但没值的字段（上游 `required` 没默认值时启用会直接报错）；返回点分路径 */
export function missingRequired(node: SNode, value: unknown, path: string[] = [], depth = 0): string[] {
    if (depth > 12 || isHidden(node)) return [];
    const v = effectiveValue(node, value);
    if (node.meta.required && (v === undefined || v === null || v === '')) return [path.join('.')];
    if (isObjectLike(node)) {
        const obj = (v && typeof v === 'object' ? v : {}) as Record<string, unknown>;
        return objectFields(node).flatMap((f) => missingRequired(f.node, obj[f.key], [...path, f.key], depth + 1));
    }
    if (node.type === 'union') {
        const shape = unionShape(node);
        if (shape.kind === 'tagged') {
            const obj = (v && typeof v === 'object' ? v : undefined) as Record<string, unknown> | undefined;
            const b = shape.branches[taggedBranch(shape, obj)];
            return b ? missingRequired(b.node, obj ?? {}, path, depth + 1) : [];
        }
    }
    return [];
}
