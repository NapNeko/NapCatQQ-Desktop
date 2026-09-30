// 把动作的参数 JSON Schema 翻成表单模型：每个字段该用哪种控件、接受什么类型的值。
//
// 两个后端给的 schema 长得不一样：NapCat 是 TypeBox 生成的（布尔参数是 anyOf[boolean, string]，
// id 一律字符串），SnowLuma 是手写的（id 是整数，带 role）。后端转换器已经把 role 统一写进
// `x-ncd-role`，这里只认这一个字段，剩下的靠 type / enum / anyOf 判断。

export type FieldKind =
    | 'text'
    | 'number'
    | 'boolean'
    | 'enum'
    | 'array'
    | 'json'
    | 'group'
    | 'friend'
    | 'member'
    | 'message_id'
    | 'message'
    | 'file'
    | 'face'
    | 'timestamp';

export interface FormField {
    name: string;
    label: string;
    description?: string;
    required: boolean;
    kind: FieldKind;
    valueType: 'string' | 'number' | 'integer' | 'boolean' | 'any';
    acceptsString: boolean;
    acceptsNumber: boolean;
    enumValues?: Array<{ value: string | number | boolean; label: string }>;
    itemKind?: 'text' | 'number';
    defaultValue?: unknown;
    minimum?: number;
    maximum?: number;
    /**
     * 这个字段「先填个空的」时该是什么：必填项初始化参数文本用。
     * 单独算出来是因为 json 字段可能是数组也可能是对象，光看 kind 分不出。
     */
    emptyValue?: unknown;
}

export interface FormModel {
    fields: FormField[];
    allowsExtra: boolean;
}

type Json = Record<string, unknown>;
type Scalar = string | number | boolean;

const isRecord = (v: unknown): v is Json => typeof v === 'object' && v !== null && !Array.isArray(v);

const ROLE_KIND: Record<string, FieldKind> = {
    group_id: 'group',
    user_id: 'friend',
    member_id: 'member',
    message_id: 'message_id',
    message: 'message',
    file: 'file',
    image: 'file',
    record: 'file',
    video: 'file',
    face_id: 'face',
    timestamp: 'timestamp',
};

/** 走「数字优先」输入规则的控件：纯数字文本能转数字就转，不然留字符串 */
const NUMERIC_TEXT_KINDS: ReadonlySet<FieldKind> = new Set(['group', 'friend', 'member', 'message_id', 'face', 'timestamp']);

/** allOf 里的片段并进来，外层自己写的键优先；只做浅合并，参数 schema 里 allOf 很少见 */
function flatten(schema: Json): Json {
    const parts = schema.allOf;
    if (!Array.isArray(parts)) return schema;
    const { allOf: _allOf, ...own } = schema;
    return Object.assign({}, ...parts.filter(isRecord), own);
}

function branchesOf(schema: Json): Json[] {
    const list = schema.anyOf ?? schema.oneOf;
    if (!Array.isArray(list)) return [];
    return list.filter(isRecord).map(flatten);
}

function jsonTypeOfValue(v: unknown): string {
    if (typeof v === 'string') return 'string';
    if (typeof v === 'boolean') return 'boolean';
    if (typeof v === 'number') return Number.isInteger(v) ? 'integer' : 'number';
    if (v === null) return 'null';
    return Array.isArray(v) ? 'array' : 'object';
}

/** 收集 schema（含 anyOf 各分支）声明的类型；没写 type 但有 properties / items / enum / const 的按内容推 */
function collectTypes(schema: Json, out: Set<string>, depth = 0): void {
    if (depth > 4) return;
    const t = schema.type;
    if (typeof t === 'string') out.add(t);
    else if (Array.isArray(t)) for (const x of t) if (typeof x === 'string') out.add(x);
    if (typeof t === 'undefined') {
        if (isRecord(schema.properties)) out.add('object');
        else if (schema.items !== undefined) out.add('array');
        else if (Array.isArray(schema.enum)) for (const v of schema.enum) out.add(jsonTypeOfValue(v));
        else if ('const' in schema) out.add(jsonTypeOfValue(schema.const));
    }
    for (const b of branchesOf(schema)) collectTypes(b, out, depth + 1);
}

function isScalar(v: unknown): v is Scalar {
    return typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean';
}

/**
 * 枚举项：`enum` 数组，或者 anyOf 里每个分支都是 const（TypeBox 的 Union of Literal）。
 * 只要有一个分支不是常量（比如 anyOf[const, string]），就不算枚举，按普通类型处理。
 */
function enumEntriesOf(schema: Json, depth = 0): Array<{ value: Scalar; label: string }> | null {
    if (depth > 3) return null;
    if (Array.isArray(schema.enum) && schema.enum.length > 0) {
        return schema.enum.filter(isScalar).map((value) => ({ value, label: String(value) }));
    }
    if ('const' in schema && isScalar(schema.const)) {
        const label = typeof schema.title === 'string' && schema.title ? schema.title : String(schema.const);
        return [{ value: schema.const, label }];
    }
    const branches = branchesOf(schema);
    if (branches.length === 0) return null;
    const parts = branches.map((b) => enumEntriesOf(b, depth + 1));
    if (parts.some((p) => p === null || p.length === 0)) return null;
    return parts.flat() as Array<{ value: Scalar; label: string }>;
}

/** 顶层和各分支里第一个出现的值；anyOf 的描述、默认值常写在分支里 */
function firstDefined<T>(schema: Json, pick: (s: Json) => T | undefined): T | undefined {
    const own = pick(schema);
    if (own !== undefined) return own;
    for (const b of branchesOf(schema)) {
        const v = pick(b);
        if (v !== undefined) return v;
    }
    return undefined;
}

const asString = (v: unknown) => (typeof v === 'string' && v !== '' ? v : undefined);
const asNumber = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : undefined);

function numericValueType(types: ReadonlySet<string>): 'number' | 'integer' {
    return types.has('number') ? 'number' : 'integer';
}

function enumValueType(values: Scalar[]): FormField['valueType'] {
    if (values.every((v) => typeof v === 'boolean')) return 'boolean';
    if (values.every((v) => typeof v === 'string')) return 'string';
    if (values.every((v) => typeof v === 'number')) return values.every(Number.isInteger) ? 'integer' : 'number';
    return 'any';
}

function itemKindOf(schema: Json): 'text' | 'number' | null {
    // items 可能写在 anyOf 的数组分支里（anyOf[array, null]）
    const holder = schema.items !== undefined ? schema : branchesOf(schema).find((b) => b.items !== undefined);
    const items = holder?.items;
    if (!isRecord(items)) return null;
    const types = new Set<string>();
    collectTypes(flatten(items), types);
    if (types.has('object') || types.has('array') || types.size === 0) return null;
    if (types.has('integer') || types.has('number')) return 'number';
    return 'text';
}

function fieldFor(name: string, rawSchema: unknown, required: boolean): FormField {
    const schema = isRecord(rawSchema) ? flatten(rawSchema) : {};
    const types = new Set<string>();
    collectTypes(schema, types);
    // nullable 的写法（type: ['string','null']）只是允许 null，不影响该用哪种控件
    if (types.size > 1) types.delete('null');
    const hasObject = types.has('object');
    const hasArray = types.has('array');
    const hasString = types.has('string');
    const hasNumber = types.has('integer') || types.has('number');
    const enumEntries = enumEntriesOf(schema);
    const roleRaw = firstDefined(schema, (s) => asString(s['x-ncd-role']));
    const roleKind = roleRaw ? ROLE_KIND[roleRaw] : undefined;
    // 角色只对能装标量的字段有效；「user_ids: 数组」这类字段即便被按名字猜成 user_id 也不该变成单个好友选择器
    const roleApplies =
        roleKind !== undefined &&
        (roleKind === 'message' || types.size === 0 || hasString || hasNumber);

    let kind: FieldKind;
    let itemKind: 'text' | 'number' | undefined;
    if (roleKind && roleApplies) {
        kind = roleKind;
    } else if (enumEntries && enumEntries.length > 0) {
        kind = 'enum';
    } else if (!hasObject && !hasArray && types.has('boolean')) {
        kind = 'boolean';
    } else if (!hasObject && !hasArray && hasNumber) {
        kind = 'number';
    } else if (hasArray && !hasObject && types.size === 1 && itemKindOf(schema)) {
        kind = 'array';
        itemKind = itemKindOf(schema) ?? 'text';
    } else if (hasObject || hasArray || types.size === 0 || (types.size === 1 && types.has('null'))) {
        kind = 'json';
    } else {
        kind = 'text';
    }

    let valueType: FormField['valueType'];
    if (kind === 'enum') valueType = enumValueType((enumEntries ?? []).map((e) => e.value));
    else if (kind === 'boolean') valueType = 'boolean';
    else if (kind === 'number') valueType = numericValueType(types);
    else if (NUMERIC_TEXT_KINDS.has(kind) || kind === 'file') {
        if (hasNumber && !hasString) valueType = numericValueType(types);
        else if (hasString && !hasNumber) valueType = 'string';
        else valueType = 'any';
    } else if (kind === 'text') valueType = 'string';
    else valueType = 'any';

    let acceptsString: boolean;
    let acceptsNumber: boolean;
    if (kind === 'enum') {
        const values = (enumEntries ?? []).map((e) => e.value);
        acceptsString = values.some((v) => typeof v === 'string');
        acceptsNumber = values.some((v) => typeof v === 'number');
    } else if (types.size === 0) {
        // 没声明类型就是什么都收
        acceptsString = true;
        acceptsNumber = true;
    } else {
        acceptsString = hasString;
        acceptsNumber = hasNumber;
    }

    const defaultValue = firstDefined(schema, (s) => (s.default !== undefined ? s.default : undefined));
    const field: FormField = {
        name,
        // 参数名本身就是开发者要对着上游文档找的东西，标题（title）不拿来顶替
        label: name,
        required,
        kind,
        valueType,
        acceptsString,
        acceptsNumber,
    };
    const description = firstDefined(schema, (s) => asString(s.description));
    if (description) field.description = description;
    if (kind === 'enum' && enumEntries) field.enumValues = enumEntries;
    if (itemKind) field.itemKind = itemKind;
    if (defaultValue !== undefined) field.defaultValue = defaultValue;
    const minimum = firstDefined(schema, (s) => asNumber(s.minimum));
    const maximum = firstDefined(schema, (s) => asNumber(s.maximum));
    if (minimum !== undefined) field.minimum = minimum;
    if (maximum !== undefined) field.maximum = maximum;
    field.emptyValue = emptyValueOf(field, hasArray && !hasObject);
    return field;
}

function emptyValueOf(field: FormField, arrayShaped: boolean): unknown {
    if (field.defaultValue !== undefined) return field.defaultValue;
    if (field.kind === 'enum') return field.enumValues?.[0]?.value ?? '';
    switch (field.kind) {
        case 'boolean':
            return false;
        case 'number':
            return 0;
        case 'array':
            return [];
        case 'json':
            return arrayShaped ? [] : {};
        default:
            // 各类 id / 时间戳：整数字段给 0，字符串字段给空串
            return field.valueType === 'integer' || field.valueType === 'number' ? 0 : '';
    }
}

/**
 * 控件类型的判定顺序：x-ncd-role → 枚举 → 布尔 → 数字 → 标量数组 → JSON → 文本。
 * 必填的排前面，同组内保持 schema 里的先后。
 */
export function buildFormModel(schema: Record<string, unknown> | null | undefined): FormModel {
    if (!isRecord(schema)) return { fields: [], allowsExtra: true };
    const root = flatten(schema);
    const properties = isRecord(root.properties) ? root.properties : {};
    const requiredNames = Array.isArray(root.required) ? root.required.filter((r): r is string => typeof r === 'string') : [];
    const requiredSet = new Set(requiredNames);

    const requiredFields: FormField[] = [];
    const optionalFields: FormField[] = [];
    for (const [name, prop] of Object.entries(properties)) {
        (requiredSet.has(name) ? requiredFields : optionalFields).push(fieldFor(name, prop, requiredSet.has(name)));
    }
    // required 里点了名、properties 里却没定义的：照样要用户填，只能当未知类型
    for (const name of requiredNames) {
        if (!(name in properties)) requiredFields.push(fieldFor(name, undefined, true));
    }
    return { fields: [...requiredFields, ...optionalFields], allowsExtra: root.additionalProperties !== false };
}

const NUMBER_TEXT = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i;
const DIGITS = /^\d+$/;
const SIGNED_DIGITS = /^-?\d+$/;
/**
 * 允许带负号的 id 类控件。SnowLuma 的 message_id 是有符号 32 位整数，会出现负数；
 * 群号 / QQ 号实际不会为负，但放行负号没有坏处，省得为它们再分一套规则。
 */
const SIGNED_ID_KINDS: ReadonlySet<FieldKind> = new Set(['group', 'friend', 'member', 'message_id']);

function numberFromText(raw: string): number | undefined {
    const text = raw.trim();
    if (!NUMBER_TEXT.test(text)) return undefined;
    const n = Number(text);
    return Number.isFinite(n) ? n : undefined;
}

/** 纯数字文本转数字，但超出安全整数范围的（长 id）宁可留字符串，免得悄悄丢精度 */
function digitsToNumber(raw: string, field: FormField): number | undefined {
    const text = raw.trim();
    const signed = field.kind === 'number' || SIGNED_ID_KINDS.has(field.kind);
    if (!(signed ? SIGNED_DIGITS : DIGITS).test(text)) return undefined;
    const n = Number(text);
    if (!Number.isSafeInteger(n) && field.acceptsString) return undefined;
    return n;
}

function coerceItem(itemKind: 'text' | 'number' | undefined, raw: string): unknown {
    if (itemKind === 'number') return numberFromText(raw) ?? raw;
    return raw;
}

/**
 * 表单里敲的文本换成参数值。空串表示「不填」（返回 undefined，调用方据此删掉这个键）；
 * 转不动的一律原样返回字符串，交给校验去报「应为整数」之类，而不是在这里悄悄改写用户输入。
 */
export function coerceInput(field: FormField, raw: string): unknown {
    if (raw === '') return undefined;
    switch (field.kind) {
        case 'number': {
            if (field.acceptsString && SIGNED_DIGITS.test(raw.trim())) return digitsToNumber(raw, field) ?? raw;
            return numberFromText(raw) ?? raw;
        }
        case 'boolean': {
            const t = raw.trim().toLowerCase();
            if (t === 'true') return true;
            if (t === 'false') return false;
            return raw;
        }
        case 'enum': {
            const hit = field.enumValues?.find((e) => String(e.value) === raw);
            return hit ? hit.value : raw;
        }
        case 'array': {
            const text = raw.trim();
            if (text.startsWith('[')) {
                try {
                    const parsed: unknown = JSON.parse(text);
                    if (Array.isArray(parsed)) return parsed;
                } catch {
                    // 不是合法 JSON，按下面的分隔文本处理
                }
            }
            return raw
                .split(/[\n,]/)
                .map((s) => s.trim())
                .filter((s) => s !== '')
                .map((s) => coerceItem(field.itemKind, s));
        }
        case 'json': {
            try {
                return JSON.parse(raw) as unknown;
            } catch {
                return raw;
            }
        }
        default: {
            if (NUMERIC_TEXT_KINDS.has(field.kind) && field.acceptsNumber) {
                const n = digitsToNumber(raw, field);
                if (n !== undefined) return n;
            }
            return raw;
        }
    }
}
