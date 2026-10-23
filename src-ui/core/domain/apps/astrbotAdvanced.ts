import snapshot from './astrbotSchema/snapshot.json';
import type { AstrBotInstanceConfig, AppConfigIssue } from '../../ipc/types';
import type { AstrBotConfigSchema } from '../../ipc/generated/domain/AstrBotConfigSchema';

export type JsonObject = Record<string, unknown>;
export type AstrBotMetadata = Record<
    string,
    {
        name?: string;
        invisible?: boolean;
        condition?: Record<string, unknown>;
        metadata?: Record<string, FieldMetadata>;
    }
>;
export type FieldMetadata = {
    type?: string;
    _special?: string;
    description?: string;
    hint?: string;
    secret?: boolean;
    invisible?: boolean;
    options?: unknown[];
    labels?: string[] | string;
    items?: JsonObject;
    condition?: Record<string, unknown>;
    min?: number;
    max?: number;
    step?: number;
    slider?: { min?: number; max?: number; step?: number };
    runner_defaults?: Record<string, JsonObject>;
};
export type AdvancedField = {
    path: string;
    label: string;
    hint?: string;
    metadata: FieldMetadata;
    conditions: Record<string, unknown>[];
};
export type AdvancedSection = { id: string; title: string; group: string; fields: AdvancedField[] };
export type AdvancedSchema = { defaults: JsonObject; metadata: AstrBotMetadata; live: boolean };

const BLOCKED_SEGMENTS = new Set(['__proto__', 'prototype', 'constructor']);
const MANAGED = snapshot.managed_paths;
const WEBUI_ACTIONS = new Set(['t2i_template', 'dashboard_totp_manager']);
const STRING_LIST_SPECIALS = new Set([
    'select_plugin_set',
    'select_providers',
    'select_knowledgebase',
]);
const SPECIAL_TYPES: Record<string, string> = {
    agent_runner_type: 'string',
    select_provider: 'string',
    select_provider_stt: 'string',
    select_provider_tts: 'string',
    select_persona: 'string',
    check_shipyard_connection: 'string',
    local_permission_matrix: 'object',
};
const LABELS: Record<string, string> = {
    'provider_tts_settings.use_file_service': '通过文件服务发送语音',
    'provider_tts_settings.dual_output': '同时发送文字与语音',
    'provider_tts_settings.trigger_probability': '语音回复触发概率',
    t2i_active_template: '图片回复模板',
    'dashboard.disable_access_log': '关闭 WebUI 访问日志',
};
export function isObject(value: unknown): value is JsonObject {
    return !!value && typeof value === 'object' && !Array.isArray(value);
}
export function readPath(root: unknown, path: string): unknown {
    let value = root;
    for (const part of path.split('.')) {
        if (BLOCKED_SEGMENTS.has(part) || !isObject(value)) return undefined;
        value = Object.prototype.hasOwnProperty.call(value, part) ? value[part] : undefined;
    }
    return value;
}
function writePath(root: JsonObject, path: string, value: unknown): void {
    const parts = path.split('.');
    if (parts.some((part) => !part || BLOCKED_SEGMENTS.has(part))) return;
    let node = root;
    for (const part of parts.slice(0, -1)) {
        if (!isObject(node[part])) node[part] = {};
        node = node[part] as JsonObject;
    }
    node[parts[parts.length - 1]] = value;
}
function removePath(root: JsonObject, path: string): void {
    const parts = path.split('.');
    let node = root;
    for (const part of parts.slice(0, -1)) {
        if (!isObject(node[part])) {
            delete node[part];
            return;
        }
        node = node[part] as JsonObject;
    }
    delete node[parts[parts.length - 1]];
}
export function isManagedPath(path: string): boolean {
    return MANAGED.some((owned) => path === owned || path.startsWith(owned + '.'));
}
export function projectAdvanced(root: JsonObject): JsonObject {
    const value = structuredClone(root);
    for (const path of MANAGED) removePath(value, path);
    return value;
}
export function parseAdvanced(text: string | undefined): JsonObject {
    try {
        const value: unknown = JSON.parse(text || '{}');
        return isObject(value) ? value : {};
    } catch {
        return {};
    }
}
export function fallbackAstrBotSchema(): AstrBotConfigSchema {
    return {
        defaults_json: JSON.stringify(snapshot.defaults),
        metadata_json: JSON.stringify(snapshot.metadata),
    };
}
export function resolveAdvancedSchema(live?: AstrBotConfigSchema): AdvancedSchema {
    if (live) {
        try {
            const defaults: unknown = JSON.parse(live.defaults_json);
            const metadata: unknown = JSON.parse(live.metadata_json);
            if (isObject(defaults) && isObject(metadata))
                return { defaults, metadata: metadata as AstrBotMetadata, live: true };
        } catch {
            // 上游旧版本或无效元数据仍可使用固定快照编辑。
        }
    }
    return {
        defaults: snapshot.defaults,
        metadata: snapshot.metadata as AstrBotMetadata,
        live: false,
    };
}
function translated(value: unknown, fallback: string): string {
    if (typeof value !== 'string') return fallback;
    const match = readPath(snapshot.translations, value);
    if (typeof match === 'string') return match;
    return /^[a-z_]+_group\./.test(value) ? fallback : value;
}
export function optionLabels(metadata: FieldMetadata): string[] {
    if (Array.isArray(metadata.labels)) return metadata.labels;
    const found =
        typeof metadata.labels === 'string'
            ? readPath(snapshot.translations, metadata.labels)
            : undefined;
    return Array.isArray(found) ? found.filter((v): v is string => typeof v === 'string') : [];
}
function merge(base: JsonObject, patch: JsonObject): JsonObject {
    const out = structuredClone(base);
    for (const [key, value] of Object.entries(patch)) {
        if (BLOCKED_SEGMENTS.has(key)) continue;
        const previous = out[key];
        out[key] =
            isObject(value) && isObject(previous) ? merge(previous, value) : structuredClone(value);
    }
    return out;
}
/** 为条件和默认值合成只读视图；专页拥有的值只参与显示，从不回写高级草稿。 */
export function effectiveAdvancedRoot(
    config: AstrBotInstanceConfig,
    schema: AdvancedSchema,
): JsonObject {
    const out = merge(schema.defaults, parseAdvanced(config.advanced_json));
    const ai = config.ai;
    const dedicated: [string, unknown][] = [
        ['provider_settings.enable', ai.enable],
        ['provider_settings.web_search', config.websearch.enable],
        ['provider_settings.streaming_response', ai.streaming_response],
        ['provider_settings.llm_safety_mode', ai.llm_safety_mode],
        ['provider_settings.max_context_length', ai.max_context_length],
        ['provider_settings.context_limit_reached_strategy', ai.context_limit_reached_strategy],
        ['provider_stt_settings.enable', config.stt.enable],
        ['provider_stt_settings.provider_id', config.stt.provider_id],
        ['provider_tts_settings.enable', config.tts.enable],
        ['provider_tts_settings.provider_id', config.tts.provider_id],
        ['platform_settings.unique_session', config.gates.unique_session],
        ['platform_settings.enable_id_white_list', config.gates.enable_id_white_list],
        [
            'platform_settings.friend_message_needs_wake_prefix',
            config.gates.friend_message_needs_wake_prefix,
        ],
        ['agent_runner.config.compression.max_turns', ai.max_context_length],
        ['agent_runner.config.compression.overflow_strategy', ai.context_limit_reached_strategy],
        ['agent_runner.config.persona.safety_mode', ai.llm_safety_mode],
    ];
    for (const [path, value] of dedicated) writePath(out, path, value);
    const runner = readPath(parseAdvanced(config.advanced_json), 'agent_runner.runner_type');
    writePath(out, 'agent_runner.runner_type', runner ?? ai.agent_runner_type ?? 'local');
    return out;
}
function leafPaths(value: unknown, path = ''): string[] {
    if (isObject(value) && Object.keys(value).length)
        return Object.entries(value).flatMap(([key, child]) =>
            leafPaths(child, path ? path + '.' + key : key),
        );
    return path ? [path] : [];
}
function inferMetadata(value: unknown, path: string): FieldMetadata {
    const sensitive =
        /password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credential|recovery[_-]?code/i.test(
            path,
        );
    const secret = sensitive ? { secret: true } : {};
    if (typeof value === 'boolean') return { type: 'bool' };
    if (typeof value === 'number') return { type: Number.isInteger(value) ? 'int' : 'float' };
    if (typeof value === 'string') return { type: 'string', ...secret };
    if (
        Array.isArray(value) &&
        (value.length > 0 || sensitive) &&
        value.every((v) => typeof v === 'string')
    )
        return { type: 'list', items: { type: 'string' }, ...secret };
    return { type: isObject(value) ? 'object' : 'json' };
}
function normalizeMetadata(metadata: FieldMetadata): FieldMetadata {
    const special = metadata._special;
    if (special && WEBUI_ACTIONS.has(special)) return { ...metadata, invisible: true };
    if (special && STRING_LIST_SPECIALS.has(special))
        return {
            ...metadata,
            type: 'list',
            items: { type: 'string' },
            options: undefined,
            labels: undefined,
        };
    if (special && !Object.prototype.hasOwnProperty.call(SPECIAL_TYPES, special))
        // 未知特殊控件的 type 可能只是 WebUI 占位，不能据此改写或拒绝已有值。
        return {
            ...metadata,
            type: 'json',
            items: undefined,
            options: undefined,
            labels: undefined,
            min: undefined,
            max: undefined,
            step: undefined,
            slider: undefined,
        };
    const type = special ? SPECIAL_TYPES[special] : metadata.type;
    return { ...metadata, type: type === 'dict' ? 'object' : type };
}
export function advancedSections(
    config: AstrBotInstanceConfig,
    schema: AdvancedSchema,
): AdvancedSection[] {
    const sections: AdvancedSection[] = [];
    const covered: string[] = [];
    for (const [groupId, group] of Object.entries(schema.metadata)) {
        if (!isObject(group) || !isObject(group.metadata)) continue;
        for (const [sectionId, section] of Object.entries(group.metadata ?? {})) {
            if (!isObject(section)) continue;
            const fields: AdvancedField[] = [];
            const visit = (
                items: Record<string, FieldMetadata>,
                prefix = '',
                conditions: Record<string, unknown>[] = [],
                hidden = false,
            ) => {
                for (const [key, rawMetadata] of Object.entries(items)) {
                    if (!isObject(rawMetadata) || key === 'type') continue;
                    const path = key.includes('.') || !prefix ? key : prefix + '.' + key;
                    if (isManagedPath(path)) continue;
                    const metadata = normalizeMetadata(rawMetadata);
                    const invisible = hidden || metadata.invisible === true;
                    if (invisible) covered.push(path);
                    const inherited = metadata.condition
                        ? [...conditions, metadata.condition]
                        : conditions;
                    if (
                        metadata.type === 'object' &&
                        isObject(metadata.items) &&
                        Object.values(metadata.items).every(isObject)
                    ) {
                        visit(
                            metadata.items as Record<string, FieldMetadata>,
                            path,
                            inherited,
                            invisible,
                        );
                    } else {
                        covered.push(path);
                        if (invisible) continue;
                        fields.push({
                            path,
                            label:
                                LABELS[path] ??
                                translated(
                                    metadata.description,
                                    path.split('.').slice(-1)[0] ?? path,
                                ),
                            hint: translated(metadata.hint, ''),
                            metadata,
                            conditions: inherited,
                        });
                    }
                }
            };
            visit(
                (isObject(section.items) ? section.items : {}) as Record<string, FieldMetadata>,
                '',
                [group.condition, section.condition].filter(
                    (condition): condition is Record<string, unknown> => !!condition,
                ),
                group.invisible === true || section.invisible === true,
            );
            if (fields.length)
                sections.push({
                    id: groupId + '/' + sectionId,
                    title: translated(section.description, sectionId),
                    group: translated(group.name, groupId),
                    fields,
                });
        }
    }
    const current = effectiveAdvancedRoot(config, schema);
    const extra = new Map<string, AdvancedField[]>();
    for (const path of leafPaths(current)) {
        if (isManagedPath(path) || covered.some((p) => path === p || path.startsWith(p + '.')))
            continue;
        const key = path.includes('.') ? path.split('.')[0] : 'general';
        const fields = extra.get(key) ?? [];
        const defaultValue = readPath(schema.defaults, path);
        fields.push({
            path,
            label: LABELS[path] ?? path.split('.').slice(-1)[0] ?? path,
            metadata: inferMetadata(
                defaultValue === undefined ? readPath(current, path) : defaultValue,
                path,
            ),
            conditions: [],
        });
        extra.set(key, fields);
    }
    for (const [key, fields] of extra)
        sections.push({
            id: 'extra/' + key,
            title: key === 'general' ? '其他设置' : key,
            group: '更多配置',
            fields,
        });
    return sections;
}
export function fieldVisible(field: AdvancedField, root: JsonObject): boolean {
    if (field.metadata.invisible) return false;
    return field.conditions.every((condition) =>
        Object.entries(condition).every(([path, expected]) => {
            const actual = readPath(root, path);
            return Array.isArray(expected)
                ? expected.includes(actual)
                : JSON.stringify(actual) === JSON.stringify(expected);
        }),
    );
}
export function setAdvancedField(
    config: AstrBotInstanceConfig,
    field: AdvancedField,
    value: unknown,
): AstrBotInstanceConfig {
    const next = parseAdvanced(config.advanced_json);
    if (
        field.path === 'agent_runner.runner_type' &&
        typeof value === 'string' &&
        field.metadata.runner_defaults?.[value]
    ) {
        writePath(next, 'agent_runner', {
            runner_type: value,
            config: structuredClone(field.metadata.runner_defaults[value]),
        });
    } else writePath(next, field.path, value);
    return {
        ...config,
        ...(field.path === 'agent_runner.runner_type' && typeof value === 'string'
            ? { ai: { ...config.ai, agent_runner_type: value } }
            : {}),
        advanced_json: JSON.stringify(projectAdvanced(next), null, 2),
    };
}
export function advancedValueIssues(
    config: AstrBotInstanceConfig,
    sections: AdvancedSection[],
): AppConfigIssue[] {
    const root = parseAdvanced(config.advanced_json);
    const issues: AppConfigIssue[] = [];
    for (const field of sections.flatMap((section) => section.fields)) {
        if (field.metadata.invisible) continue;
        const value = readPath(root, field.path);
        if (value === undefined) continue;
        const type = field.metadata.type;
        let message = '';
        if (
            (type === 'int' || type === 'float') &&
            (typeof value !== 'number' || !Number.isFinite(value))
        )
            message = '请输入有效数字';
        else if (type === 'int' && !Number.isInteger(value)) message = '请输入整数';
        else if (type === 'bool' && typeof value !== 'boolean') message = '需要布尔值';
        else if ((type === 'string' || type === 'text') && typeof value !== 'string')
            message = '需要文本';
        else if (
            (type === 'list' || type === 'template_list' || type === 'file') &&
            !Array.isArray(value)
        )
            message = '需要列表';
        else if (
            type === 'list' &&
            field.metadata.items?.type === 'string' &&
            Array.isArray(value) &&
            value.some((item) => typeof item !== 'string')
        )
            message = '列表项需要文本';
        else if (type === 'object' && !isObject(value)) message = '需要 JSON 对象';
        if (!message && typeof value === 'number') {
            const min = field.metadata.min ?? field.metadata.slider?.min;
            const max = field.metadata.max ?? field.metadata.slider?.max;
            if (min !== undefined && value < min) message = '不能小于 ' + min;
            if (max !== undefined && value > max) message = '不能大于 ' + max;
        }
        if (message) issues.push({ path: 'advanced/' + field.path, message });
    }
    return issues;
}
export const ASTRBOT_SCHEMA_SOURCE = snapshot.source;
