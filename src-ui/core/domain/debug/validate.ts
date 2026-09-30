// 发送前的参数校验：只覆盖调试台用得上的 JSON Schema 子集，
// 目的是在请求发出去之前把「漏填、类型错、越界」标在字段上，而不是做完整的 schema 验证器。
// 校验只提醒不拦截（界面有「仍然发送」），所以宁可漏报也别误报：不认识的关键字一律当没写。

export interface ParamIssue {
    path: string;
    message: string;
}

type Json = Record<string, unknown>;

const isRecord = (v: unknown): v is Json => typeof v === 'object' && v !== null && !Array.isArray(v);

const TYPE_LABEL: Record<string, string> = {
    string: '字符串',
    number: '数字',
    integer: '整数',
    boolean: '布尔值',
    array: '数组',
    object: '对象',
    null: '空值',
};

/** 枚举提示最多列这么多个，再多就是一大串没法读 */
const MAX_LISTED = 8;

function typeOfValue(v: unknown): string {
    if (v === null) return 'null';
    if (Array.isArray(v)) return 'array';
    if (typeof v === 'number') return Number.isInteger(v) ? 'integer' : 'number';
    return typeof v;
}

function typeMatches(declared: string, actual: string): boolean {
    if (declared === actual) return true;
    // 整数也是数字
    return declared === 'number' && actual === 'integer';
}

function declaredTypes(schema: Json): string[] {
    const t = schema.type;
    if (typeof t === 'string') return [t];
    if (Array.isArray(t)) return t.filter((x): x is string => typeof x === 'string');
    return [];
}

function typeMessage(types: string[]): string {
    const labels = types.map((t) => TYPE_LABEL[t] ?? t);
    return labels.length === 1 ? `应为${labels[0]}` : `应为 ${labels.join(' / ')} 之一`;
}

function listValues(values: unknown[]): string {
    const shown = values.slice(0, MAX_LISTED).map((v) => (typeof v === 'string' ? v : JSON.stringify(v)));
    return values.length > MAX_LISTED ? `${shown.join(' / ')} …` : shown.join(' / ');
}

function sameValue(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (typeof a === 'object' && typeof b === 'object' && a !== null && b !== null) {
        return JSON.stringify(a) === JSON.stringify(b);
    }
    return false;
}

const joinKey = (base: string, key: string) => (base ? `${base}.${key}` : key);

/** 这个分支「认不认」这种值：声明了类型的看类型，只有 const / enum 的看值，什么都没写的当认 */
function branchAccepts(branch: Json, value: unknown): boolean {
    const types = declaredTypes(branch);
    const actual = typeOfValue(value);
    if (types.length > 0) return types.some((t) => typeMatches(t, actual));
    if ('const' in branch) return sameValue(branch.const, value);
    if (Array.isArray(branch.enum)) return branch.enum.some((e) => sameValue(e, value));
    return true;
}

function describeBranches(branches: Json[]): string {
    const types = new Set<string>();
    const values: unknown[] = [];
    for (const b of branches) {
        for (const t of declaredTypes(b)) types.add(t);
        if ('const' in b) values.push(b.const);
        if (Array.isArray(b.enum)) values.push(...b.enum);
    }
    if (values.length > 0 && types.size === 0) return `应为 ${listValues(values)} 之一`;
    return typeMessage([...types]);
}

interface Ctx {
    issues: ParamIssue[];
    /** 已经在数组元素里了：元素里再嵌数组不往下查，items 只查一层 */
    inItems: boolean;
}

function validateNode(schema: Json, value: unknown, path: string, ctx: Ctx): void {
    const allOf = schema.allOf;
    if (Array.isArray(allOf)) {
        for (const part of allOf) if (isRecord(part)) validateNode(part, value, path, ctx);
    }

    const alternatives = schema.anyOf ?? schema.oneOf;
    if (Array.isArray(alternatives)) {
        const branches = alternatives.filter(isRecord);
        if (branches.length > 0) validateAnyOf(branches, value, path, ctx);
    }

    const types = declaredTypes(schema);
    const actual = typeOfValue(value);
    if (types.length > 0 && !types.some((t) => typeMatches(t, actual))) {
        ctx.issues.push({ path, message: typeMessage(types) });
        return;
    }

    if ('const' in schema && !sameValue(schema.const, value)) {
        ctx.issues.push({ path, message: `应为 ${listValues([schema.const])}` });
        return;
    }
    if (Array.isArray(schema.enum) && schema.enum.length > 0 && !schema.enum.some((e) => sameValue(e, value))) {
        ctx.issues.push({ path, message: `应为 ${listValues(schema.enum)} 之一` });
        return;
    }

    if (typeof value === 'number') {
        if (typeof schema.minimum === 'number' && value < schema.minimum) {
            ctx.issues.push({ path, message: `不能小于 ${schema.minimum}` });
        }
        if (typeof schema.maximum === 'number' && value > schema.maximum) {
            ctx.issues.push({ path, message: `不能大于 ${schema.maximum}` });
        }
        return;
    }

    if (Array.isArray(value)) {
        const items = schema.items;
        if (!ctx.inItems && isRecord(items)) {
            const inner: Ctx = { issues: ctx.issues, inItems: true };
            value.forEach((el, i) => validateNode(items, el, `${path}[${i}]`, inner));
        }
        return;
    }

    if (isRecord(value)) validateObject(schema, value, path, ctx);
}

function validateAnyOf(branches: Json[], value: unknown, path: string, ctx: Ctx): void {
    let firstAccepting: ParamIssue[] | null = null;
    for (const branch of branches) {
        const trial: Ctx = { issues: [], inItems: ctx.inItems };
        validateNode(branch, value, path, trial);
        if (trial.issues.length === 0) return;
        // 类型对得上、只是内容不合格的分支，它的报错最有参考价值（比如数字越界）
        if (firstAccepting === null && branchAccepts(branch, value)) firstAccepting = trial.issues;
    }
    if (firstAccepting) ctx.issues.push(...firstAccepting);
    else ctx.issues.push({ path, message: describeBranches(branches) });
}

function isMissing(value: unknown): boolean {
    // 空串在表单里等于「没填」，必填项填了空串同样算漏填
    return value === undefined || value === null || value === '';
}

function validateObject(schema: Json, value: Json, path: string, ctx: Ctx): void {
    const properties = isRecord(schema.properties) ? schema.properties : {};
    if (Array.isArray(schema.required)) {
        for (const name of schema.required) {
            if (typeof name === 'string' && isMissing(value[name])) {
                ctx.issues.push({ path: joinKey(path, name), message: '必填' });
            }
        }
    }
    for (const [key, v] of Object.entries(value)) {
        // 只认 schema 自己声明的键：`constructor`、`__proto__` 这类原型上的名字不能当成已声明
        const declared = Object.prototype.hasOwnProperty.call(properties, key);
        const prop = declared ? properties[key] : undefined;
        if (isRecord(prop)) {
            // 必填的空值上面已经报过「必填」，不再叠一条类型错误
            if (isMissing(v) && Array.isArray(schema.required) && schema.required.includes(key)) continue;
            validateNode(prop, v, joinKey(path, key), ctx);
        } else if (schema.additionalProperties === false && !declared) {
            ctx.issues.push({ path: joinKey(path, key), message: '不支持这个参数' });
        }
    }
}

/**
 * 按参数 schema 检查一份参数对象。支持：required、type（含类型数组）、anyOf / oneOf（任一分支通过即可）、
 * allOf、const、enum、minimum / maximum、items（只查一层）、additionalProperties: false。
 */
export function validateParams(
    schema: Record<string, unknown> | null | undefined,
    value: Record<string, unknown>,
): ParamIssue[] {
    if (!isRecord(schema)) return [];
    const ctx: Ctx = { issues: [], inItems: false };
    validateObject(schema, value, '', ctx);
    return ctx.issues;
}
