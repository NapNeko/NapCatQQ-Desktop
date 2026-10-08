// 联合类型字段：枚举下拉 / 标签联合卡片 / 混合分支取可画的支。

import { Select } from '../../../../../shared/ui';
import {
    effectiveValue,
    mixedBranch,
    renderable,
    unionShape,
    type SNode,
} from '../../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText } from '../schemaLabel';
import { asObj } from '../schemasteryModel';
import { SchemaField } from './SchemaField';
import { NestedCard } from './NestedCard';
import { InlineUnion } from './InlineUnion';
import { JsonField } from './JsonField';

export function UnionField({
    name,
    node,
    value,
    onChange,
    disabled,
    canReset,
}: {
    name: string;
    node: SNode;
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
    canReset: boolean;
}) {
    const shape = unionShape(node);
    const shown = effectiveValue(node, value);
    const reset = canReset ? () => onChange(undefined) : undefined;
    const { hint } = fieldText(name, node);
    if (shape.kind === 'enum') {
        const index = shape.options.findIndex((o) => o.value === shown);
        return (
            <Select
                label={<FieldLabel name={name} node={node} disabled={disabled} onReset={reset} />}
                hint={hint}
                value={index >= 0 ? String(index) : undefined}
                placeholder="未设置"
                items={shape.options.map((o, i) => ({ value: String(i), label: o.label }))}
                disabled={disabled}
                onValueChange={(v) => onChange(shape.options[Number(v)]?.value)}
            />
        );
    }
    if (shape.kind === 'tagged') {
        return (
            <NestedCard name={name} node={node} onReset={reset} disabled={disabled}>
                <InlineUnion
                    node={node}
                    value={asObj(shown)}
                    onChange={onChange}
                    disabled={disabled}
                />
            </NestedCard>
        );
    }
    const branches = shape.branches.filter((b) => renderable(b));
    if (branches.length === 0)
        return (
            <JsonField
                name={name}
                node={node}
                value={value}
                onChange={onChange}
                disabled={disabled}
            />
        );
    const branch = branches[mixedBranch(branches, shown)] ?? branches[0];
    // 联合自己的说明、默认值、必填挂到选中的分支上显示
    const merged: SNode = { ...branch, meta: { ...branch.meta, ...node.meta } };
    return (
        <SchemaField
            name={name}
            node={merged}
            value={value}
            onChange={onChange}
            disabled={disabled}
        />
    );
}
