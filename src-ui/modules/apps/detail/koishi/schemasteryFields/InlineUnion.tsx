// 交叉里的标签联合：判别键下拉 + 选中分支的字段平铺在同一张表上。

import { Select } from '../../../../../shared/ui';
import { cn } from '../../../../../shared/utils/cn';
import {
    describe,
    isObjectLike,
    taggedBranch,
    unionShape,
    type SNode,
} from '../../../../../core/domain/apps/koishiSchema';
import { WIDE, stripKey, type Obj } from '../schemasteryModel';
import { SchemaObject } from './SchemaObject';

/** 交叉里的一段标签联合（如 OneBot 适配器按 protocol 分三种连法）：判别键和分支字段写在同一张表上 */
export function InlineUnion({
    node,
    value,
    onChange,
    disabled,
}: {
    node: SNode;
    value: Obj;
    onChange: (next: Obj) => void;
    disabled?: boolean;
}) {
    const shape = unionShape(node);
    if (shape.kind !== 'tagged') {
        const b = shape.kind === 'mixed' ? shape.branches.find(isObjectLike) : undefined;
        return b ? (
            <div className={WIDE}>
                <SchemaObject node={b} value={value} onChange={onChange} disabled={disabled} />
            </div>
        ) : null;
    }
    const index = taggedBranch(shape, value);
    const branch = shape.branches[index];
    return (
        <div className={cn('flex flex-col gap-4', WIDE)}>
            <div className="grid gap-x-5 sm:grid-cols-2">
                <Select
                    label={
                        <span className="flex items-center gap-1.5">
                            连接方式
                            <span className="font-mono text-2xs font-normal text-text-disabled">
                                {shape.key}
                            </span>
                        </span>
                    }
                    hint={describe(node) || undefined}
                    value={String(index)}
                    items={shape.branches.map((b, i) => ({ value: String(i), label: b.label }))}
                    disabled={disabled}
                    onValueChange={(v) => {
                        const next = shape.branches[Number(v)];
                        if (next) onChange({ ...value, [shape.key]: next.value });
                    }}
                />
            </div>
            {branch && (
                <SchemaObject
                    node={stripKey(branch.node, shape.key)}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                />
            )}
        </div>
    );
}
