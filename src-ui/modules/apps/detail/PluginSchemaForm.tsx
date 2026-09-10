// 插件配置表单：按框架 schema 翻译出的字段树渲染；值仍是那份 JSON 文档，保存走原文写入。
// 桌面端不认的类型（`json`）和非字符串列表退回 JSON 源码编辑，避免把用户的值改形。

import { useEffect, useState, type ReactNode } from 'react';
import {
    FormSection,
    NumberField,
    Select,
    StringListField,
    Switch,
    SyntaxTextEditor,
    TextField,
} from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import type { AppPluginConfigField } from '../../../core/ipc/types';

export type PluginConfigObject = Record<string, unknown>;

function isObject(v: unknown): v is PluginConfigObject {
    return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function isStringList(v: unknown): v is string[] {
    return Array.isArray(v) && v.every((x) => typeof x === 'string');
}

function hintNode(field: AppPluginConfigField): ReactNode {
    if (!field.hint) return undefined;
    return field.obvious_hint ? <span className="text-warning">{field.hint}</span> : field.hint;
}

const TextAreaField: React.FC<{
    label: ReactNode;
    hint?: ReactNode;
    value: string;
    disabled?: boolean;
    onValueChange: (v: string) => void;
}> = ({ label, hint, value, disabled, onValueChange }) => (
    <div className="flex flex-col gap-1.5">
        <label className="text-xs font-medium text-text-secondary">{label}</label>
        <textarea
            value={value}
            disabled={disabled}
            rows={4}
            onChange={(e) => onValueChange(e.target.value)}
            className={cn(
                'block w-full resize-y rounded-sm border border-border-subtle bg-field px-3 py-2 font-mono text-sm text-text',
                'outline-none transition-colors duration-150 placeholder:text-text-tertiary',
                'focus:border-brand focus:ring-2 focus:ring-brand focus:ring-inset',
                'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
            )}
        />
        {hint && <p className="text-2xs leading-snug text-text-tertiary">{hint}</p>}
    </div>
);

/** 任意 JSON 值的源码编辑；只有能解析时才回写，避免半截输入把值改坏。 */
const JsonValueField: React.FC<{
    label: ReactNode;
    hint?: ReactNode;
    value: unknown;
    disabled?: boolean;
    onValueChange: (v: unknown) => void;
}> = ({ label, hint, value, disabled, onValueChange }) => {
    const canonical = JSON.stringify(value ?? null, null, 2);
    const [text, setText] = useState(canonical);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setText((cur) => {
            try {
                if (JSON.stringify(JSON.parse(cur)) === JSON.stringify(value ?? null)) return cur;
            } catch {
                // 本地正在编辑的非法文本不被外部值覆盖
                return cur;
            }
            return canonical;
        });
    }, [canonical, value]);

    return (
        <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-text-secondary">{label}</label>
            <div className="flex h-32 flex-col">
                <SyntaxTextEditor
                    mode="json"
                    wrap
                    value={text}
                    disabled={disabled}
                    invalid={!!error}
                    aria-label={typeof label === 'string' ? label : 'JSON'}
                    onChange={(next) => {
                        setText(next);
                        try {
                            onValueChange(JSON.parse(next));
                            setError(null);
                        } catch (e) {
                            setError((e as Error).message);
                        }
                    }}
                />
            </div>
            {(error || hint) && (
                <p className={cn('text-2xs leading-snug', error ? 'text-danger' : 'text-text-tertiary')}>
                    {error ? `JSON 语法错误：${error}` : hint}
                </p>
            )}
        </div>
    );
};

const FieldControl: React.FC<{
    field: AppPluginConfigField;
    value: unknown;
    disabled?: boolean;
    onValueChange: (v: unknown) => void;
}> = ({ field, value, disabled, onValueChange }) => {
    const hint = hintNode(field);
    switch (field.kind) {
        case 'string': {
            const str = typeof value === 'string' ? value : value == null ? '' : String(value);
            if (field.options.length > 0) {
                const items = field.options.map((o) => ({ value: o, label: o }));
                if (str && !field.options.includes(str)) items.unshift({ value: str, label: str });
                return (
                    <Select
                        label={field.label}
                        hint={hint}
                        items={items}
                        value={str || undefined}
                        disabled={disabled}
                        onValueChange={onValueChange}
                    />
                );
            }
            return (
                <TextField
                    label={field.label}
                    hint={hint}
                    type={field.secret ? 'password' : 'text'}
                    autoComplete="off"
                    value={str}
                    disabled={disabled}
                    className={field.secret ? 'font-mono' : undefined}
                    onValueChange={onValueChange}
                />
            );
        }
        case 'text':
            return (
                <TextAreaField
                    label={field.label}
                    hint={hint}
                    value={typeof value === 'string' ? value : value == null ? '' : String(value)}
                    disabled={disabled}
                    onValueChange={onValueChange}
                />
            );
        case 'int':
        case 'float':
            return (
                <NumberField
                    label={field.label}
                    hint={hint}
                    allowFloat={field.kind === 'float'}
                    value={typeof value === 'number' ? value : null}
                    disabled={disabled}
                    onValueChange={(n) => onValueChange(n ?? 0)}
                />
            );
        case 'bool':
            return (
                <Switch
                    label={field.label}
                    hint={hint}
                    checked={value === true}
                    disabled={disabled}
                    onCheckedChange={onValueChange}
                />
            );
        case 'list':
            if (value == null || isStringList(value)) {
                return (
                    <StringListField
                        label={field.label}
                        hint={hint}
                        value={(value as string[] | null) ?? []}
                        disabled={disabled}
                        mono={false}
                        onChange={onValueChange}
                    />
                );
            }
            return (
                <JsonValueField
                    label={field.label}
                    hint={hint}
                    value={value}
                    disabled={disabled}
                    onValueChange={onValueChange}
                />
            );
        case 'object': {
            const child = isObject(value) ? value : {};
            return (
                <FormSection title={field.label} description={field.hint || undefined}>
                    <PluginSchemaFields
                        fields={field.items}
                        value={child}
                        disabled={disabled}
                        onChange={onValueChange}
                    />
                </FormSection>
            );
        }
        case 'json':
        default:
            return (
                <JsonValueField
                    label={field.label}
                    hint={hint}
                    value={value}
                    disabled={disabled}
                    onValueChange={onValueChange}
                />
            );
    }
};

export const PluginSchemaFields: React.FC<{
    fields: AppPluginConfigField[];
    value: PluginConfigObject;
    disabled?: boolean;
    onChange: (next: PluginConfigObject) => void;
}> = ({ fields, value, disabled, onChange }) => (
    <div className="flex flex-col gap-4">
        {fields.map((field) => (
            <FieldControl
                key={field.key}
                field={field}
                value={value[field.key]}
                disabled={disabled}
                onValueChange={(v) => onChange({ ...value, [field.key]: v })}
            />
        ))}
    </div>
);

export const PluginSchemaForm: React.FC<{
    fields: AppPluginConfigField[];
    value: PluginConfigObject;
    disabled?: boolean;
    onChange: (next: PluginConfigObject) => void;
}> = ({ fields, value, disabled, onChange }) => {
    if (fields.length === 0) {
        return <p className="text-sm text-text-secondary">这个插件没有可配置项。</p>;
    }
    return (
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            <PluginSchemaFields fields={fields} value={value} disabled={disabled} onChange={onChange} />
        </div>
    );
};
