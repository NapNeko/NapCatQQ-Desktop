// 目录生成：手写定义按后端折算成完整 DebugActionSpec，并汇总成目录；差异由两边结果直接比出来。

import type { BackendType } from '../../generated/domain/BackendType';
import type { DebugActionSpec } from '../../generated/debug/DebugActionSpec';
import type { DebugActionSummary } from '../../generated/debug/DebugActionSummary';
import type { DebugCatalog } from '../../generated/debug/DebugCatalog';
import type { DebugCatalogSource } from '../../generated/debug/DebugCatalogSource';
import type { DebugParamDiff } from '../../generated/debug/DebugParamDiff';
import { ID_ROLES, type ParamDef, reply, type ActionDef } from './catalog-schema';
import { DEFS_ACCOUNT } from './catalog-defs-account';
import { DEFS_MESSAGE } from './catalog-defs-message';
import { DEFS_GROUP_INFO, DEFS_GROUP_ADMIN } from './catalog-defs-group';
import { DEFS_FRIEND, DEFS_REQUEST } from './catalog-defs-friend';
import {
    DEFS_FILE,
    DEFS_FACE,
    DEFS_STREAM,
    DEFS_EXTENSION,
    DEFS_PREVIEW,
} from './catalog-defs-file';

// ---------------------------------------------------------------------------
// 动作定义
// ---------------------------------------------------------------------------

const DEFS: ActionDef[] = [
    ...DEFS_ACCOUNT,
    ...DEFS_MESSAGE,
    ...DEFS_GROUP_INFO,
    ...DEFS_GROUP_ADMIN,
    ...DEFS_FRIEND,
    ...DEFS_REQUEST,
    ...DEFS_FILE,
    ...DEFS_FACE,
    ...DEFS_STREAM,
    ...DEFS_EXTENSION,
    ...DEFS_PREVIEW,
];

// ---------------------------------------------------------------------------
// 生成
// ---------------------------------------------------------------------------

const otherOf = (backend: BackendType): BackendType =>
    backend === 'napcat' ? 'snowluma' : 'napcat';
const hasBackend = (def: ActionDef, backend: BackendType) =>
    !def.backends || def.backends.includes(backend);
const isRequired = (p: ParamDef, backend: BackendType) =>
    p.requiredOn?.[backend] ?? p.required === true;
const visibleParams = (def: ActionDef, backend: BackendType) =>
    def.params.filter((p) => !p.only || p.only === backend);

function propertySchema(p: ParamDef, backend: BackendType): Record<string, unknown> {
    const type = p.typeOn?.[backend] ?? p.type;
    let schema: Record<string, unknown>;
    if (p.role && ID_ROLES.has(p.role)) {
        // NapCat 的 id 一律按字符串收，SnowLuma 收整数
        schema = backend === 'napcat' ? { type: 'string' } : { type: 'integer' };
    } else if (p.role === 'message') {
        schema = {
            ...(backend === 'napcat' ? { $id: 'OB11MessageMixType' } : {}),
            anyOf: [{ type: 'array', items: { type: 'object' } }, { type: 'string' }],
        };
    } else if (p.values) {
        schema = { type: typeof p.values[0] === 'number' ? 'integer' : 'string', enum: p.values };
    } else if (type === 'boolean') {
        // TypeBox 生成的布尔参数：NapCat 同时接受字符串 "true" / "false"
        schema =
            backend === 'napcat'
                ? { anyOf: [{ type: 'boolean' }, { type: 'string' }] }
                : { type: 'boolean' };
    } else if (type === 'array') {
        schema = { type: 'array', items: { type: 'object' } };
    } else if (type === 'object') {
        schema = { type: 'object' };
    } else {
        schema = { type: type ?? 'string' };
    }
    if (p.role) schema['x-ncd-role'] = p.role;
    schema.description = p.desc;
    if (p.default !== undefined) schema.default = p.default;
    if (p.minimum !== undefined) schema.minimum = p.minimum;
    if (p.maximum !== undefined) schema.maximum = p.maximum;
    return schema;
}

function paramsSchema(def: ActionDef, backend: BackendType): Record<string, unknown> {
    const params = visibleParams(def, backend);
    const required = params.filter((p) => isRequired(p, backend)).map((p) => p.name);
    return {
        type: 'object',
        properties: Object.fromEntries(params.map((p) => [p.name, propertySchema(p, backend)])),
        ...(required.length > 0 ? { required } : {}),
    };
}

/** 给人看的类型写法 */
function typeText(schema: Record<string, unknown>): string {
    if (typeof schema.type === 'string') return schema.type;
    const any = schema.anyOf;
    if (Array.isArray(any)) {
        return any.map((s) => String((s as Record<string, unknown>).type ?? 'any')).join(' | ');
    }
    return 'any';
}

/**
 * 比较用的类型。和后端转换器同一口径：id 类的字符串 / 整数算同一种；布尔的 `boolean` 和
 * `boolean|string` 写法算同一种；`integer` 是 `number` 的子集，比较前并成同一个
 * （NapCat 把 `duration` 写成 number，SnowLuma 写成 integer，这不是差异）
 */
function comparableType(schema: Record<string, unknown>): string {
    const role = schema['x-ncd-role'];
    if (typeof role === 'string' && ID_ROLES.has(role)) return 'id';
    if (role === 'message') return 'message';
    const any = schema.anyOf;
    if (Array.isArray(any) && any.some((s) => (s as Record<string, unknown>).type === 'boolean'))
        return 'boolean';
    return typeText(schema).replace(/\binteger\b/g, 'number');
}

/** 真正必须由调用方给出的参数：列在 required 里且没写 default（NapCat 会把带默认值的也列进去） */
function requiredWithoutDefault(
    props: Record<string, Record<string, unknown>>,
    required: Set<string>,
): Set<string> {
    const out = new Set<string>();
    for (const name of required) {
        const prop = props[name];
        if (prop !== undefined && prop.default === undefined) out.add(name);
    }
    return out;
}

/**
 * 两边同名参数的出入，以及「照一边的写法发到另一边会不会直接失败」。
 * `breaking` 的口径和 Rust 的 `params_incompatible` 一致：同名参数类型大类不同，
 * 或者某一侧必填、另一侧根本没有这个参数。只是必填与否不同、或只有一侧有的可选参数不算。
 */
function diffParams(
    def: ActionDef,
    here: BackendType,
): { diffs: DebugParamDiff[]; breaking: boolean } {
    const there = otherOf(here);
    const a = paramsSchema(def, here);
    const b = paramsSchema(def, there);
    const propsA = (a.properties ?? {}) as Record<string, Record<string, unknown>>;
    const propsB = (b.properties ?? {}) as Record<string, Record<string, unknown>>;
    const reqA = new Set((a.required ?? []) as string[]);
    const reqB = new Set((b.required ?? []) as string[]);
    const strictA = requiredWithoutDefault(propsA, reqA);
    const strictB = requiredWithoutDefault(propsB, reqB);
    const diffs: DebugParamDiff[] = [];
    let typeClash = false;
    for (const name of Object.keys(propsA)) {
        const other = propsB[name];
        const mine = propsA[name] as Record<string, unknown>;
        if (!other) {
            diffs.push({ name, diff: { kind: 'only_here' } });
        } else if (comparableType(mine) !== comparableType(other)) {
            typeClash = true;
            diffs.push({
                name,
                diff: { kind: 'type_differs', here: typeText(mine), other: typeText(other) },
            });
        } else if (reqA.has(name) !== reqB.has(name)) {
            diffs.push({
                name,
                diff: { kind: 'required_differs', here: reqA.has(name), other: reqB.has(name) },
            });
        }
    }
    for (const name of Object.keys(propsB)) {
        if (!propsA[name]) diffs.push({ name, diff: { kind: 'only_other' } });
    }
    const lacksRequired =
        [...strictA].some((name) => propsB[name] === undefined) ||
        [...strictB].some((name) => propsA[name] === undefined);
    return { diffs, breaking: typeClash || lacksRequired };
}

function buildBase(def: ActionDef, backend: BackendType): DebugActionSpec {
    const napcat = backend === 'napcat';
    const otherPresent = hasBackend(def, otherOf(backend));
    // 对方没有这个接口时按「没有」算：diff 为空，也不叫不兼容
    const diff = otherPresent ? diffParams(def, backend) : { diffs: [], breaking: false };
    return {
        name: def.name,
        aliases: def.aliases ?? [],
        summary: def.summary,
        description: napcat ? (def.description ?? null) : null,
        category: def.category,
        safety: def.safety,
        stream: def.stream === true,
        supported: true,
        params_schema: paramsSchema(def, backend),
        // SnowLuma 的 `returns` 常常只是一段话；写了 returnsText 的动作它就不给结构
        returns_schema: def.returns && (napcat || !def.returnsText) ? def.returns : null,
        returns_text: !napcat ? (def.returnsText ?? null) : null,
        return_example: napcat && def.returnData !== undefined ? reply(def.returnData) : null,
        examples: napcat && def.example ? [def.example] : [],
        error_examples: napcat ? (def.errorExamples ?? []) : [],
        invariants: napcat ? [] : (def.invariants ?? []),
        other_backend: {
            backend: otherOf(backend),
            present: otherPresent,
            diffs: diff.diffs,
            breaking: diff.breaking,
        },
        source: 'snapshot',
    };
}

const baseCache = new Map<BackendType, DebugActionSpec[]>();

function baseSpecs(backend: BackendType): DebugActionSpec[] {
    let list = baseCache.get(backend);
    if (!list) {
        list = DEFS.filter((d) => hasBackend(d, backend)).map((d) => buildBase(d, backend));
        baseCache.set(backend, list);
    }
    return list;
}

export interface MockSpecOptions {
    source: DebugCatalogSource;
    /** 当前 Bot 没实现的动作名（老版本上游缺的），进「当前 Bot 不支持」分组 */
    unsupported?: ReadonlySet<string>;
}

/** 按名字或别名取一个动作的完整说明；这个后端没有就返回 null */
export function buildMockSpec(
    backend: BackendType,
    nameOrAlias: string,
    opts: MockSpecOptions,
): DebugActionSpec | null {
    const base = baseSpecs(backend).find(
        (s) => s.name === nameOrAlias || s.aliases.includes(nameOrAlias),
    );
    if (!base) return null;
    return { ...base, supported: !opts.unsupported?.has(base.name), source: opts.source };
}

function summarize(spec: DebugActionSpec): DebugActionSummary {
    return {
        name: spec.name,
        aliases: spec.aliases,
        summary: spec.summary,
        category: spec.category,
        safety: spec.safety,
        stream: spec.stream,
        supported: spec.supported,
        other_backend_present: spec.other_backend ? spec.other_backend.present : null,
        // 只有真的不兼容才带徽章；只是必填不同、或只有一侧有的可选参数，只在文档页的对照表里列
        param_diff: spec.other_backend?.breaking ?? false,
    };
}

export function buildMockCatalog(backend: BackendType, opts: MockSpecOptions): DebugCatalog {
    return {
        backend,
        source: opts.source,
        snapshot_version: backend === 'napcat' ? '4.15.18' : '0.9.0',
        actions: baseSpecs(backend).map((base) =>
            summarize({
                ...base,
                supported: !opts.unsupported?.has(base.name),
                source: opts.source,
            }),
        ),
    };
}

/** 一个动作在这个后端上的必填参数名；动作不存在返回 null */
export function mockRequiredParams(backend: BackendType, name: string): string[] | null {
    const def = DEFS.find((d) => d.name === name && hasBackend(d, backend));
    if (!def) return null;
    return visibleParams(def, backend)
        .filter((p) => isRequired(p, backend))
        .map((p) => p.name);
}
