// 一段字段的网格：空键是内嵌的标签联合，其余一个键一个控件。

import { withKey, type Obj } from '../schemasteryModel';
import type { SNode } from '../../../../../core/domain/apps/koishiSchema';
import { SchemaField } from './SchemaField';
import { InlineUnion } from './InlineUnion';

export function FieldGrid({
    fields,
    value,
    onChange,
    disabled,
}: {
    fields: { key: string; node: SNode }[];
    value: Obj;
    onChange: (next: Obj) => void;
    disabled?: boolean;
}) {
    return (
        <div className="grid min-w-0 gap-x-5 gap-y-4 sm:grid-cols-2">
            {fields.map((f, i) =>
                f.key === '' ? (
                    <InlineUnion
                        key={`u${i}`}
                        node={f.node}
                        value={value}
                        onChange={onChange}
                        disabled={disabled}
                    />
                ) : (
                    <SchemaField
                        key={f.key}
                        name={f.key}
                        node={f.node}
                        value={value[f.key]}
                        disabled={disabled}
                        onChange={(v) => onChange(withKey(value, f.key, v))}
                    />
                ),
            )}
        </div>
    );
}
