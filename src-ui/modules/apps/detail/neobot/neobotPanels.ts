// 面板其余几页的收窄器：插件 / 模型 / 提示词 / 记忆。
//
// 与 neobotPanel.ts 同一套原则——面板无 schema 承诺，一律防御式解析：
// 缺键给默认值、类型不符就丢，绝不直接断言。键名取自后端 handler（api.py / config_manager.py），
// 不是猜的。

import { asBool, asNumber, asRecord, asString } from './neobotPanel';

/** 面板列表回包共用形状：一个对象，里面一个数组字段 */
function listOf(raw: unknown, key: string): Record<string, unknown>[] | null {
    const r = asRecord(raw);
    if (!r) return null;
    const arr = r[key];
    if (!Array.isArray(arr)) return null;
    return arr
        .map((x) => asRecord(x))
        .filter((x): x is Record<string, unknown> => x !== null);
}

function asStringArray(v: unknown): string[] {
    if (!Array.isArray(v)) return [];
    return v.filter((x): x is string => typeof x === 'string');
}

// ---------------------------------------------------------------- 插件

export interface NeoBotPlugin {
    id: string;
    name: string;
    version: string;
    status: string;
    enabled: boolean;
    description: string;
    author: string;
    official: boolean;
    /** 面板是否允许对它做启停 / 卸载（官方插件通常不可卸） */
    manageable: boolean;
    error: string;
    repo: string;
    homepage: string;
    tags: string[];
    /** 缺的 Python 依赖；非空说明装了也跑不起来 */
    missingPythonDependencies: string[];
    configPath: string;
}

export interface NeoBotPlugins {
    items: NeoBotPlugin[];
    /** 面板是否开着管理功能（关了就只能看，不能启停 / 装） */
    manageEnabled: boolean;
    /** 面板自己；它不能被卸载 */
    consolePlugin: string;
}

export function parseNeoBotPlugins(raw: unknown): NeoBotPlugins | null {
    const items = listOf(raw, 'items');
    const r = asRecord(raw);
    if (items === null || !r) return null;
    return {
        items: items.map((p) => ({
            id: asString(p.id) || asString(p.name),
            name: asString(p.name) || asString(p.id),
            version: asString(p.version),
            status: asString(p.status),
            enabled: asBool(p.enabled),
            description: asString(p.description),
            author: asString(p.author),
            official: asBool(p.official),
            manageable: asBool(p.manageable),
            error: asString(p.error),
            repo: asString(p.repo),
            homepage: asString(p.homepage),
            tags: asStringArray(p.tags),
            missingPythonDependencies: asStringArray(p.missing_python_dependencies),
            configPath: asString(p.config_path),
        })),
        manageEnabled: asBool(r.manage_enabled),
        consolePlugin: asString(r.console_plugin),
    };
}

// ---------------------------------------------------------------- 模型

export interface NeoBotModel {
    /** 本机引用名（配置里用它），不是发给供应商的模型名 */
    modelRef: string;
    displayName: string;
    provider: string;
    modelName: string;
    typeLabel: string;
    /** 该供应商的 Key 是否已配；没配这个模型用不了 */
    providerHasKey: boolean;
}

export interface NeoBotModels {
    library: NeoBotModel[];
    /** 角色 → 模型引用名（如 chat / vision） */
    assignments: Record<string, string>;
}

export function parseNeoBotModels(raw: unknown): NeoBotModels | null {
    const r = asRecord(raw);
    if (!r || !Array.isArray(r.library)) return null;
    // 平台凭据状态在 platforms 里，用来标「这个模型现在能不能用」
    const platforms = asRecord(r.platforms) ?? {};
    const hasKey = (provider: string): boolean => {
        const p = asRecord(platforms[provider]);
        return p ? asBool(p.has_key) : false;
    };
    const assignments: Record<string, string> = {};
    const assignRaw = asRecord(r.assignments) ?? {};
    for (const [role, ref] of Object.entries(assignRaw)) {
        if (typeof ref === 'string' && ref.trim()) assignments[role] = ref;
    }
    return {
        library: r.library
            .map((x) => asRecord(x))
            .filter((x): x is Record<string, unknown> => x !== null)
            .map((m) => {
                const provider = asString(m.provider);
                return {
                    modelRef: asString(m.model_ref),
                    displayName: asString(m.display_name),
                    provider,
                    modelName: asString(m.model_name),
                    typeLabel: asString(m.type_label) || asString(m.model_type) || 'chat',
                    providerHasKey: hasKey(provider),
                };
            })
            .filter((m) => m.modelRef !== ''),
        assignments,
    };
}

// ---------------------------------------------------------------- 提示词

export interface NeoBotPromptKey {
    path: string;
    label: string;
    kind: string;
    /** 合并后的实际取值（默认值被自定义值覆盖后的结果） */
    value: string;
    /** 是否被自定义值覆盖过（面板据此标「已改」） */
    overridden: boolean;
    placeholders: string[];
}

export interface NeoBotPromptSection {
    name: string;
    keys: NeoBotPromptKey[];
}

export interface NeoBotPrompts {
    sections: NeoBotPromptSection[];
    editable: boolean;
}

export function parseNeoBotPrompts(raw: unknown): NeoBotPrompts | null {
    const r = asRecord(raw);
    if (!r || !Array.isArray(r.sections)) return null;
    return {
        sections: r.sections
            .map((s) => asRecord(s))
            .filter((s): s is Record<string, unknown> => s !== null)
            .map((s) => ({
                name: asString(s.name) || asString(s.title),
                keys: (Array.isArray(s.keys) ? s.keys : [])
                    .map((k) => asRecord(k))
                    .filter((k): k is Record<string, unknown> => k !== null)
                    .map((k) => ({
                        path: asString(k.path),
                        label: asString(k.label) || asString(k.path),
                        kind: asString(k.kind, 'scalar'),
                        value: typeof k.value === 'string' ? k.value : '',
                        overridden: asBool(k.overridden),
                        placeholders: asStringArray(k.placeholders),
                    }))
                    .filter((k) => k.path !== ''),
            }))
            .filter((s) => s.name !== ''),
        editable: asBool(r.editable),
    };
}

// ---------------------------------------------------------------- 记忆（档案表）

export interface NeoBotArchiveTable {
    name: string;
    count: number;
    /** 超过长度上限的条目数；0 表示都还在限内 */
    overLimit: number;
}

export interface NeoBotArchives {
    tables: NeoBotArchiveTable[];
    summarizeAvailable: boolean;
}

export function parseNeoBotArchives(raw: unknown): NeoBotArchives | null {
    const items = listOf(raw, 'items');
    const r = asRecord(raw);
    if (items === null || !r) return null;
    return {
        tables: items
            .map((t) => ({
                // 列表接口用 table_name，超限清单用 name —— 两个都认
                name: asString(t.table_name) || asString(t.name),
                count: asNumber(t.count),
                overLimit: asNumber(t.over_limit),
            }))
            .filter((t) => t.name !== ''),
        summarizeAvailable: asBool(r.summarize_available),
    };
}
