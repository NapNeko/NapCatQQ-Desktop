import { useState, type ReactNode } from 'react';
import { Checkbox, Select, TextAreaField, TextField } from '../../../../shared/ui';
import {
    json,
    record,
    text,
    type NeoBotField,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';

export function JsonDraftField({
    label,
    value,
    onChange,
    onValidity,
    disabled,
}: {
    label: string;
    value: unknown;
    onChange: (value: unknown) => void;
    onValidity?: (valid: boolean) => void;
    disabled?: boolean;
}) {
    const [edit, setEdit] = useState<string | null>(null);
    const [error, setError] = useState('');
    return (
        <TextAreaField
            label={label}
            value={edit ?? json(value)}
            mono
            minRows={3}
            maxRows={16}
            disabled={disabled}
            error={error}
            onValueChange={(source) => {
                setEdit(source);
                try {
                    const parsed: unknown = JSON.parse(source);
                    onChange(parsed);
                    setError('');
                    onValidity?.(true);
                } catch {
                    setError('JSON 格式不正确');
                    onValidity?.(false);
                }
            }}
        />
    );
}

function NumericField({
    field,
    label,
    value,
    disabled,
    onChange,
    onValidity,
}: {
    field: NeoBotField;
    label: ReactNode;
    value: unknown;
    disabled?: boolean;
    onChange: (value: number) => void;
    onValidity: (valid: boolean) => void;
}) {
    const [draft, setDraft] = useState<string | null>(null);
    const [error, setError] = useState('');
    return (
        <TextField
            label={label}
            type="number"
            step={field.type === 'int' ? 1 : 'any'}
            min={field.min}
            max={field.max}
            value={draft ?? text(value)}
            disabled={disabled}
            error={error}
            onValueChange={(source) => {
                setDraft(source);
                const number = Number(source);
                const valid =
                    source.trim() !== '' &&
                    Number.isFinite(number) &&
                    (field.type !== 'int' || Number.isInteger(number)) &&
                    (field.min === undefined || number >= field.min) &&
                    (field.max === undefined || number <= field.max);
                setError(valid ? '' : '请输入范围内的有效数字');
                onValidity(valid);
                if (valid) onChange(number);
            }}
        />
    );
}

export function NeoBotSchemaFields({
    fields,
    value,
    onChange,
    onValidity,
    disabled,
    prefix = '',
}: {
    fields: NeoBotField[];
    value: PanelObject;
    onChange: (value: PanelObject) => void;
    onValidity?: (path: string, valid: boolean) => void;
    disabled?: boolean;
    prefix?: string;
}) {
    return (
        <div className="flex flex-col gap-4">
            {fields.map((f) => {
                const current = value[f.name] ?? f.value ?? f.defaultValue;
                const path = prefix ? `${prefix}.${f.name}` : f.name;
                const set = (next: unknown) => onChange({ ...value, [f.name]: next });
                const blocked = disabled || f.readonly;
                const label = <span title={f.description}>{f.label}</span>;
                if (f.kind === 'group')
                    return (
                        <details
                            key={f.name}
                            open
                            className="rounded-sm border border-border-subtle p-3"
                        >
                            <summary
                                className="mb-3 cursor-pointer text-xs font-medium text-text"
                                title={f.description}
                            >
                                {f.label}
                            </summary>
                            <NeoBotSchemaFields
                                fields={f.fields}
                                value={record(current)}
                                disabled={blocked}
                                prefix={path}
                                onValidity={onValidity}
                                onChange={set}
                            />
                        </details>
                    );
                if (typeof current === 'boolean' || f.type === 'bool')
                    return (
                        <Checkbox
                            key={f.name}
                            label={label}
                            checked={current === true}
                            disabled={blocked}
                            onCheckedChange={set}
                        />
                    );
                if (f.options.length && f.strict)
                    return (
                        <Select
                            key={f.name}
                            label={label}
                            items={f.options.filter(Boolean).map((v) => ({ value: v, label: v }))}
                            value={text(current)}
                            disabled={blocked}
                            onValueChange={set}
                        />
                    );
                if (
                    ['list', 'dict', 'model_list', 'model_params'].includes(f.kind) ||
                    (typeof current === 'object' && current !== null)
                )
                    return (
                        <JsonDraftField
                            key={f.name}
                            label={f.label}
                            value={current}
                            disabled={blocked}
                            onChange={set}
                            onValidity={(valid) => onValidity?.(path, valid)}
                        />
                    );
                if (typeof current === 'number' || /^(int|float)$/.test(f.type))
                    return (
                        <NumericField
                            key={f.name}
                            field={f}
                            label={label}
                            value={current}
                            disabled={blocked}
                            onChange={set}
                            onValidity={(valid) => onValidity?.(path, valid)}
                        />
                    );
                return text(current).includes('\n') ||
                    /prompt|persona|data|description/.test(f.name) ? (
                    <TextAreaField
                        key={f.name}
                        label={label}
                        value={text(current)}
                        disabled={blocked}
                        onValueChange={set}
                    />
                ) : (
                    <TextField
                        key={f.name}
                        label={label}
                        type={f.sensitive ? 'password' : 'text'}
                        autoComplete="off"
                        value={text(current)}
                        disabled={blocked}
                        onValueChange={set}
                    />
                );
            })}
        </div>
    );
}
