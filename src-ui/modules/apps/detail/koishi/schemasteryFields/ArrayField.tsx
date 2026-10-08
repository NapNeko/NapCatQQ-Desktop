// 数组字段：字符串数组走标签输入，其余每项一张行。

import { Plus } from 'lucide-react';
import { Button, StringListField } from '../../../../../shared/ui';
import { blankOf, isObjectLike, type SNode } from '../../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText } from '../schemaLabel';
import { WIDE, asObj } from '../schemasteryModel';
import { SchemaField } from './SchemaField';
import { SchemaObject } from './SchemaObject';
import { NestedCard } from './NestedCard';
import { ItemRemove } from './ItemRemove';

export function ArrayField({
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
    const list = Array.isArray(value) ? value : [];
    const inner = node.inner;
    if (!inner) return null;
    const reset = canReset ? () => onChange(undefined) : undefined;
    if (inner.type === 'string') {
        // 空串是有意义的一项（Koishi 的指令前缀默认 [''] = 不带前缀也能触发），标签输入画不出空标签，单独说明、原样留着
        const hasEmpty = list.some((x) => x === '');
        const hint = [fieldText(name, node).hint, hasEmpty ? '另含一个空值（不带前缀也算）' : '']
            .filter(Boolean)
            .join(' ');
        return (
            <StringListField
                className={WIDE}
                label={<FieldLabel name={name} node={node} disabled={disabled} onReset={reset} />}
                hint={hint || '输入后回车添加一项'}
                value={list.map(String).filter((x) => x !== '')}
                disabled={disabled}
                onChange={(next) => onChange(hasEmpty ? ['', ...next] : next)}
            />
        );
    }
    const add = (
        <Button
            size="sm"
            variant="secondary"
            disabled={disabled}
            onClick={() => onChange([...list, blankOf(inner)])}
        >
            <Plus size={13} />
            加一项
        </Button>
    );
    return (
        <NestedCard
            name={name}
            node={node}
            onReset={reset}
            disabled={disabled}
            actions={add}
            count={list.length}
        >
            {list.length === 0 ? (
                <p className="py-1 text-xs text-text-tertiary">还没有，点「加一项」</p>
            ) : (
                <ul className="flex flex-col divide-y divide-border-subtle/70">
                    {list.map((item, i) => (
                        <li key={i} className="flex items-start gap-3 py-3 first:pt-0 last:pb-0">
                            <span className="mt-7 w-5 shrink-0 text-right text-2xs tabular-nums text-text-disabled">
                                {i + 1}
                            </span>
                            <div className="grid min-w-0 flex-1 gap-3 sm:grid-cols-2">
                                {isObjectLike(inner) ? (
                                    <div className={WIDE}>
                                        <SchemaObject
                                            node={inner}
                                            value={asObj(item)}
                                            disabled={disabled}
                                            onChange={(v) =>
                                                onChange(list.map((x, j) => (j === i ? v : x)))
                                            }
                                        />
                                    </div>
                                ) : (
                                    <SchemaField
                                        name=""
                                        node={inner}
                                        value={item}
                                        resettable={false}
                                        disabled={disabled}
                                        onChange={(v) =>
                                            onChange(list.map((x, j) => (j === i ? v : x)))
                                        }
                                    />
                                )}
                            </div>
                            <ItemRemove
                                disabled={disabled}
                                onClick={() => onChange(list.filter((_, j) => j !== i))}
                            />
                        </li>
                    ))}
                </ul>
            )}
        </NestedCard>
    );
}
