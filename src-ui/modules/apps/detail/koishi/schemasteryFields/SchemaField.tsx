// 单字段控件分发：按 schema 节点类型挑控件，复合类型转给对应的子件。

import { NumberField, Switch, TextAreaField, TextField } from '../../../../../shared/ui';
import { effectiveValue, isHidden, renderable } from '../../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText } from '../schemaLabel';
import { WIDE, asObj, type FieldProps } from '../schemasteryModel';
import { NestedCard } from './NestedCard';
import { UnionField } from './UnionField';
import { ArrayField } from './ArrayField';
import { DictField } from './DictField';
import { JsonField } from './JsonField';
import { SchemaObject } from './SchemaObject';

/** 一个字段：按类型挑控件 */
export function SchemaField({
    name,
    node,
    value,
    onChange,
    disabled,
    resettable = true,
}: FieldProps) {
    if (isHidden(node)) return null;
    const shown = effectiveValue(node, value);
    const canReset = resettable && value !== undefined && node.meta.default !== undefined;
    const label = (
        <FieldLabel
            name={name}
            node={node}
            disabled={disabled}
            onReset={canReset ? () => onChange(undefined) : undefined}
        />
    );
    const { hint } = fieldText(name, node);

    if (!renderable(node))
        return (
            <JsonField
                name={name}
                node={node}
                value={value}
                onChange={onChange}
                disabled={disabled}
            />
        );

    switch (node.type) {
        case 'boolean':
            return (
                <div className="flex min-h-[58px] items-center rounded-md border border-border-subtle/70 bg-surface px-3 py-2.5">
                    <Switch
                        label={label}
                        hint={hint}
                        checked={shown === true}
                        disabled={disabled}
                        onCheckedChange={onChange}
                    />
                </div>
            );
        case 'number':
        case 'natural':
        case 'percent': {
            const step = node.meta.step ?? (node.type === 'percent' ? 0.01 : undefined);
            const isInt = node.type === 'natural' || (step !== undefined && Number.isInteger(step));
            const range = node.type === 'percent' ? '0 到 1 之间' : '';
            return (
                <NumberField
                    label={label}
                    hint={[hint, range].filter(Boolean).join(' ') || undefined}
                    value={typeof shown === 'number' ? shown : null}
                    min={node.meta.min ?? (node.type === 'natural' ? 0 : undefined)}
                    max={node.meta.max}
                    step={step}
                    allowFloat={!isInt}
                    disabled={disabled}
                    onValueChange={(n) => onChange(n === null ? undefined : n)}
                />
            );
        }
        case 'const':
            return (
                <TextField
                    label={label}
                    hint={hint}
                    value={String(node.value ?? '')}
                    disabled
                    readOnly
                />
            );
        case 'string':
        case 'date': {
            const text =
                typeof shown === 'string'
                    ? shown
                    : shown === undefined || shown === null
                      ? ''
                      : String(shown);
            if (node.meta.role === 'textarea') {
                return (
                    <TextAreaField
                        className={WIDE}
                        label={label}
                        hint={hint}
                        value={text}
                        minRows={3}
                        disabled={disabled}
                        onValueChange={(v) => onChange(v)}
                    />
                );
            }
            return (
                <TextField
                    label={label}
                    hint={hint}
                    value={text}
                    type={node.meta.role === 'secret' ? 'password' : 'text'}
                    placeholder={
                        node.meta.default !== undefined && node.meta.default !== ''
                            ? String(node.meta.default)
                            : undefined
                    }
                    disabled={disabled}
                    onValueChange={(v) => onChange(v === '' && !node.meta.required ? undefined : v)}
                />
            );
        }
        case 'union':
            return (
                <UnionField
                    name={name}
                    node={node}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                    canReset={canReset}
                />
            );
        case 'object':
        case 'intersect':
            return (
                <NestedCard
                    name={name}
                    node={node}
                    onReset={canReset ? () => onChange(undefined) : undefined}
                    disabled={disabled}
                >
                    <SchemaObject
                        node={node}
                        value={asObj(shown)}
                        onChange={onChange}
                        disabled={disabled}
                    />
                </NestedCard>
            );
        case 'array':
            return (
                <ArrayField
                    name={name}
                    node={node}
                    value={shown}
                    onChange={onChange}
                    disabled={disabled}
                    canReset={canReset}
                />
            );
        case 'dict':
            return (
                <DictField
                    name={name}
                    node={node}
                    value={shown}
                    onChange={onChange}
                    disabled={disabled}
                    canReset={canReset}
                />
            );
        case 'tuple': {
            const arr = Array.isArray(shown) ? shown : [];
            return (
                <NestedCard
                    name={name}
                    node={node}
                    onReset={canReset ? () => onChange(undefined) : undefined}
                    disabled={disabled}
                >
                    <div className="grid gap-3 sm:grid-cols-2">
                        {(node.list ?? []).map((item, i) => (
                            <SchemaField
                                key={i}
                                name={`${i + 1}`}
                                node={item}
                                value={arr[i]}
                                resettable={false}
                                disabled={disabled}
                                onChange={(v) => {
                                    const next = [...arr];
                                    next[i] = v;
                                    onChange(next);
                                }}
                            />
                        ))}
                    </div>
                </NestedCard>
            );
        }
        case 'transform':
            return node.inner ? (
                <SchemaField
                    name={name}
                    node={node.inner}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                />
            ) : null;
        default:
            return (
                <JsonField
                    name={name}
                    node={node}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                />
            );
    }
}
