// 上游面板载荷的收窄与编辑草稿工具。
import { asRecord, asString } from './neobotPanel';

export type PanelObject = Record<string, unknown>;
export const record = (raw: unknown): PanelObject => asRecord(raw) ?? {};
export const records = (raw: unknown): PanelObject[] =>
    Array.isArray(raw) ? raw.map(asRecord).filter((x): x is PanelObject => x !== null) : [];
export const strings = (raw: unknown): string[] =>
    Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : [];
export const text = (raw: unknown): string =>
    typeof raw === 'number' || typeof raw === 'boolean' ? String(raw) : asString(raw);
export const json = (raw: unknown): string => JSON.stringify(raw ?? null, null, 2);

export class NeoBotPanelError extends Error {
    constructor(
        message: string,
        readonly status?: number,
        readonly data?: unknown,
    ) {
        super(message);
        this.name = 'NeoBotPanelError';
    }
}

export interface NeoBotField {
    name: string;
    path: string[];
    kind: string;
    type: string;
    label: string;
    description: string;
    value: unknown;
    defaultValue: unknown;
    options: string[];
    strict: boolean;
    readonly: boolean;
    hidden: boolean;
    sensitive: boolean;
    fields: NeoBotField[];
    min?: number;
    max?: number;
}

export function parseFields(raw: unknown): NeoBotField[] {
    return records(raw)
        .map((f) => ({
            name: text(f.name),
            path: strings(f.path),
            kind: text(f.kind),
            type: text(f.type),
            label: text(f.label) || text(f.name),
            description: text(f.description),
            value: f.value,
            defaultValue: f.default,
            options: strings(f.options),
            strict: f.options_strict === true,
            readonly: f.readonly === true,
            hidden: f.hidden === true,
            sensitive: f.sensitive === true,
            fields: parseFields(f.fields),
            min: typeof f.min === 'number' ? f.min : undefined,
            max: typeof f.max === 'number' ? f.max : undefined,
        }))
        .filter((f) => f.name && !f.hidden);
}

export function defaultsFor(fields: NeoBotField[]): PanelObject {
    return Object.fromEntries(
        fields
            .filter((f) => !f.sensitive)
            .map((f) => [
                f.name,
                f.kind === 'group'
                    ? defaultsFor(f.fields)
                    : structuredClone(f.defaultValue ?? f.value ?? ''),
            ]),
    );
}

export function fieldErrors(raw: unknown): string {
    return records(record(raw).errors)
        .map((e) => `${text(e.path)}：${text(e.message)}`)
        .join('\n');
}

export function changeMessage(raw: PanelObject, reloadRequested = false): string {
    const changes = record(raw.changes);
    const restart = records(changes.needs_restart).map((x) => text(x.path));
    restart.push(...strings(raw.needs_restart_parts));
    const suffix = restart.length
        ? `；需重启：${restart.join('、')}`
        : raw.needs_restart === true
          ? '；部分配置需重启'
          : '';
    const reload =
        reloadRequested && raw.applied === false ? '；已保存，但运行期重载未成功，请检查日志' : '';
    return (text(raw.message) || '操作完成') + suffix + reload;
}

export function paramsPath(
    path: string,
    values: Record<string, string | number | boolean | undefined>,
): string {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(values)) {
        if (value !== undefined && value !== '') params.set(key, String(value));
    }
    return params.size ? `${path}?${params}` : path;
}

export function modelAssignments(raw: unknown): PanelObject {
    const assignments = record(raw);
    const roles = record(assignments.roles);
    return {
        ...(Object.keys(roles).length ? roles : assignments),
        ...(Array.isArray(assignments.creator_image_models)
            ? { creator_image_models: assignments.creator_image_models }
            : {}),
    };
}
