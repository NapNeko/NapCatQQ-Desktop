import { useMemo, useState } from 'react';
import {
    Button,
    FormSection,
    JsonCodeEditor,
    NumberField,
    Select,
    StringListField,
    Switch,
    TextField,
} from '../../../../shared/ui';
import {
    ASTRBOT_SCHEMA_SOURCE,
    advancedSections,
    advancedValueIssues,
    effectiveAdvancedRoot,
    fieldVisible,
    isObject,
    optionLabels,
    parseAdvanced,
    projectAdvanced,
    readPath,
    resolveAdvancedSchema,
    setAdvancedField,
    type AdvancedField,
} from '../../../../core/domain/apps/astrbotAdvanced';
import { useAstrBotConfigSchema } from '../../../../hooks/apps/useAstrBotDashboard';
import type { AstrBotInstanceConfig } from '../../../../core/ipc/types';

export type AdvancedDrafts = Readonly<Record<string, string>>;

export function AstrBotAdvancedTab({
    instanceId,
    live,
    config,
    onChange,
    drafts,
    onDraft,
    disabled,
}: {
    instanceId: string;
    live: boolean;
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    drafts: AdvancedDrafts;
    onDraft: (path: string, text: string | null) => void;
    disabled?: boolean;
}) {
    const remote = useAstrBotConfigSchema(instanceId, live);
    const schema = useMemo(() => resolveAdvancedSchema(remote.data), [remote.data]);
    const sections = useMemo(() => advancedSections(config, schema), [config, schema]);
    const root = useMemo(() => effectiveAdvancedRoot(config, schema), [config, schema]);
    const issues = useMemo(() => advancedValueIssues(config, sections), [config, sections]);
    const [search, setSearch] = useState('');
    const query = search.trim().toLocaleLowerCase();
    const shown = sections
        .map((section) => ({
            ...section,
            fields: section.fields.filter((field) =>
                query
                    ? [field.path, field.label, field.hint ?? '']
                          .join(' ')
                          .toLocaleLowerCase()
                          .includes(query)
                    : fieldVisible(field, root),
            ),
        }))
        .filter((section) => section.fields.length);
    const update = (field: AdvancedField, value: unknown) =>
        onChange(setAdvancedField(config, field, value));
    const jsonChange = (field: AdvancedField, text: string) => {
        try {
            const value: unknown = JSON.parse(text);
            onDraft(field.path, null);
            update(field, value);
        } catch {
            onDraft(field.path, text);
        }
    };

    return (
        <div className="flex flex-col gap-5">
            <div className="flex flex-wrap items-end justify-between gap-3">
                <TextField
                    className="min-w-64 flex-1"
                    label="搜索高级设置"
                    placeholder="名称、说明或配置路径"
                    value={search}
                    onValueChange={setSearch}
                />
                {live && (
                    <Button
                        variant="ghost"
                        size="sm"
                        disabled={remote.isFetching}
                        onClick={() => void remote.refetch()}
                    >
                        刷新配置说明
                    </Button>
                )}
            </div>
            <p className="text-xs text-text-secondary">
                {schema.live
                    ? '配置说明来自当前实例'
                    : '使用内置 AstrBot ' + ASTRBOT_SCHEMA_SOURCE.version + ' 配置说明'}
                {' · ' +
                    sections.reduce((sum, section) => sum + section.fields.length, 0) +
                    ' 项高级设置'}
            </p>
            {Object.keys(drafts).length > 0 && (
                <div
                    role="alert"
                    className="rounded-md border border-danger p-3 text-sm text-danger"
                >
                    {Object.keys(drafts).map((path) => (
                        <button
                            key={path}
                            type="button"
                            className="block text-left underline"
                            onClick={() => setSearch(path === '$raw' ? '' : path)}
                        >
                            {(path === '$raw' ? '完整高级配置 JSON' : path) + '：JSON 格式有误'}
                        </button>
                    ))}
                </div>
            )}
            {shown.map((section) => (
                <details
                    key={section.id}
                    open={query ? true : undefined}
                    className="rounded-md border border-border-subtle bg-panel"
                >
                    <summary className="cursor-pointer px-4 py-3 text-sm font-medium">
                        <span className="mr-2 text-text-secondary">{section.group}</span>
                        {section.title}
                        <span className="ml-2 text-xs text-text-tertiary">
                            {section.fields.length}
                        </span>
                    </summary>
                    <div className="px-4 pb-4">
                        <FormSection title={section.title}>
                            {section.fields.map((field) => {
                                const value = readPath(root, field.path);
                                const metadata = field.metadata;
                                const hint = [field.hint, field.path].filter(Boolean).join(' · ');
                                const labels = optionLabels(metadata);
                                if (metadata.options?.length) {
                                    const options = metadata.options;
                                    const index = options.findIndex(
                                        (option) =>
                                            JSON.stringify(option) === JSON.stringify(value),
                                    );
                                    return (
                                        <Select
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            value={index >= 0 ? String(index) : 'current'}
                                            items={[
                                                ...(index < 0
                                                    ? [
                                                          {
                                                              value: 'current',
                                                              label:
                                                                  value === undefined
                                                                      ? '未设置'
                                                                      : String(value),
                                                          },
                                                      ]
                                                    : []),
                                                ...options.map((option, i) => ({
                                                    value: String(i),
                                                    label: labels[i] ?? String(option),
                                                })),
                                            ]}
                                            disabled={disabled}
                                            onValueChange={(selected) => {
                                                const item = Number(selected);
                                                if (
                                                    Number.isInteger(item) &&
                                                    item >= 0 &&
                                                    item < options.length
                                                )
                                                    update(field, options[item]);
                                            }}
                                        />
                                    );
                                }
                                if (metadata.type === 'bool')
                                    return (
                                        <Switch
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            checked={value === true}
                                            disabled={disabled}
                                            onCheckedChange={(next) => update(field, next)}
                                        />
                                    );
                                if (metadata.type === 'int' || metadata.type === 'float')
                                    return (
                                        <NumberField
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            value={typeof value === 'number' ? value : 0}
                                            min={metadata.min ?? metadata.slider?.min}
                                            max={metadata.max ?? metadata.slider?.max}
                                            step={
                                                metadata.step ??
                                                metadata.slider?.step ??
                                                (metadata.type === 'int' ? 1 : 0.01)
                                            }
                                            allowFloat={metadata.type === 'float'}
                                            disabled={disabled}
                                            onValueChange={(next) => {
                                                if (next !== null) update(field, next);
                                            }}
                                        />
                                    );
                                if (metadata.type === 'text')
                                    return (
                                        <label
                                            key={field.path}
                                            className="flex flex-col gap-2 text-sm"
                                        >
                                            <span className="font-medium">{field.label}</span>
                                            <textarea
                                                rows={6}
                                                className="rounded-sm border border-border-subtle bg-field px-3 py-2 text-text"
                                                value={typeof value === 'string' ? value : ''}
                                                disabled={disabled}
                                                onChange={(event) =>
                                                    update(field, event.currentTarget.value)
                                                }
                                            />
                                            <span className="text-xs text-text-secondary">
                                                {hint}
                                            </span>
                                        </label>
                                    );
                                if (metadata.type === 'string')
                                    return (
                                        <TextField
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            value={typeof value === 'string' ? value : ''}
                                            type={metadata.secret ? 'password' : 'text'}
                                            disabled={disabled}
                                            onValueChange={(next) => update(field, next)}
                                        />
                                    );
                                const stringList =
                                    metadata.type === 'list' &&
                                    (metadata.items?.type === 'string' ||
                                        (!metadata.items?.type &&
                                            Array.isArray(value) &&
                                            value.length > 0 &&
                                            value.every((item) => typeof item === 'string')));
                                if (stringList) {
                                    const values = Array.isArray(value)
                                        ? value.filter(
                                              (item): item is string => typeof item === 'string',
                                          )
                                        : [];
                                    return metadata.secret ? (
                                        <SecretList
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            value={values}
                                            disabled={disabled}
                                            onChange={(next) => update(field, next)}
                                        />
                                    ) : (
                                        <StringListField
                                            key={field.path}
                                            label={field.label}
                                            hint={hint}
                                            value={values}
                                            disabled={disabled}
                                            onChange={(next) => update(field, next)}
                                        />
                                    );
                                }
                                const draft = drafts[field.path];
                                return (
                                    <div key={field.path} className="flex flex-col gap-2">
                                        <div className="text-sm font-medium">{field.label}</div>
                                        <p className="text-xs text-text-secondary">{hint}</p>
                                        <JsonCodeEditor
                                            className="h-52 min-h-40 flex-none"
                                            ariaLabel={field.label}
                                            value={
                                                draft ??
                                                JSON.stringify(
                                                    value ?? (metadata.type === 'object' ? {} : []),
                                                    null,
                                                    2,
                                                )
                                            }
                                            readOnly={disabled}
                                            onChange={(text) => jsonChange(field, text)}
                                        />
                                        {draft !== undefined && (
                                            <p role="alert" className="text-xs text-danger">
                                                JSON 格式有误，修正后才能保存
                                            </p>
                                        )}
                                    </div>
                                );
                            })}
                        </FormSection>
                        {issues
                            .filter((issue) =>
                                section.fields.some(
                                    (field) => issue.path === 'advanced/' + field.path,
                                ),
                            )
                            .map((issue) => (
                                <p
                                    key={issue.path}
                                    role="alert"
                                    className="mt-2 text-xs text-danger"
                                >
                                    {issue.path.slice(9) + '：' + issue.message}
                                </p>
                            ))}
                    </div>
                </details>
            ))}
            {shown.length === 0 && (
                <p className="py-6 text-center text-sm text-text-secondary">没有匹配的配置项</p>
            )}
            <details className="rounded-md border border-border-subtle px-4 py-3">
                <summary className="cursor-pointer text-sm">完整高级配置 JSON</summary>
                <div className="mt-3 flex flex-col gap-2">
                    <JsonCodeEditor
                        className="h-96 flex-none"
                        ariaLabel="完整高级配置 JSON"
                        value={
                            drafts.$raw ??
                            JSON.stringify(parseAdvanced(config.advanced_json), null, 2)
                        }
                        readOnly={disabled}
                        onChange={(text) => {
                            try {
                                const value: unknown = JSON.parse(text);
                                if (!isObject(value)) throw new Error('object required');
                                onDraft('$raw', null);
                                onChange({
                                    ...config,
                                    advanced_json: JSON.stringify(projectAdvanced(value), null, 2),
                                });
                            } catch {
                                onDraft('$raw', text);
                            }
                        }}
                    />
                    {drafts.$raw !== undefined && (
                        <p role="alert" className="text-xs text-danger">
                            请输入有效的 JSON 对象
                        </p>
                    )}
                </div>
            </details>
        </div>
    );
}

function SecretList({
    label,
    hint,
    value,
    onChange,
    disabled,
}: {
    label: string;
    hint: string;
    value: string[];
    onChange: (next: string[]) => void;
    disabled?: boolean;
}) {
    return (
        <div className="flex flex-col gap-2">
            <div className="text-sm font-medium">{label}</div>
            <p className="text-xs text-text-secondary">{hint}</p>
            {value.map((item, i) => (
                <div key={i} className="flex items-center gap-2">
                    <TextField
                        className="flex-1"
                        type="password"
                        aria-label={label + ' ' + (i + 1)}
                        value={item}
                        disabled={disabled}
                        onValueChange={(next) =>
                            onChange(value.map((old, index) => (index === i ? next : old)))
                        }
                    />
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={disabled}
                        onClick={() => onChange(value.filter((_, index) => index !== i))}
                    >
                        移除
                    </Button>
                </div>
            ))}
            <Button
                size="sm"
                variant="ghost"
                className="self-start"
                disabled={disabled}
                onClick={() => onChange([...value, ''])}
            >
                添加密钥
            </Button>
        </div>
    );
}
