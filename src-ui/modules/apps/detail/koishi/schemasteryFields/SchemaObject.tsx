// 对象字段的排布：顶层（sections）按段画成带竖条标题的小节，嵌套的直接排成网格。

import { FormSection } from '../../../../../shared/ui';
import {
    effectiveValue,
    isHidden,
    objectFields,
    objectSections,
    type SNode,
} from '../../../../../core/domain/apps/koishiSchema';
import {
    asObj,
    expandUnions,
    mergeSections,
    unwrapSection,
    withKey,
    type Obj,
} from '../schemasteryModel';
import { FieldGrid } from './FieldGrid';

/** 对象的所有字段；顶层（sections）按段画成带竖条标题的小节，嵌套的直接排成网格 */
export function SchemaObject({
    node,
    value,
    onChange,
    disabled,
    sections = false,
}: {
    node: SNode;
    value: Obj;
    onChange: (next: Obj) => void;
    disabled?: boolean;
    sections?: boolean;
}) {
    if (!sections) {
        const parts = objectSections(node)
            .map(unwrapSection)
            .filter((s) => s.fields.length > 0);
        return (
            <div className="flex flex-col gap-4">
                {parts.map((s, i) => (
                    <FieldGrid
                        key={i}
                        fields={s.fields}
                        value={value}
                        onChange={onChange}
                        disabled={disabled}
                    />
                ))}
            </div>
        );
    }
    const parts = mergeSections(expandUnions(objectSections(node), value))
        .map(unwrapSection)
        .filter((s) => s.fields.length > 0);
    return (
        <div className="flex flex-col gap-8">
            {parts.map((s, i) => {
                const inner = s.inner;
                return (
                    <FormSection
                        key={i}
                        title={
                            s.title || (parts.length === 1 ? '设置' : i === 0 ? '基础设置' : '其它')
                        }
                        layout="none"
                    >
                        {inner ? (
                            <FieldGrid
                                fields={objectFields(inner.node)
                                    .filter((f) => !isHidden(f.node))
                                    .concat(
                                        inner.node.type === 'intersect'
                                            ? objectSections(inner.node).flatMap((x) =>
                                                  x.fields.filter((f) => f.key === ''),
                                              )
                                            : [],
                                    )}
                                value={asObj(effectiveValue(inner.node, value[inner.key]))}
                                onChange={(v) => onChange(withKey(value, inner.key, v))}
                                disabled={disabled}
                            />
                        ) : (
                            <FieldGrid
                                fields={s.fields}
                                value={value}
                                onChange={onChange}
                                disabled={disabled}
                            />
                        )}
                    </FormSection>
                );
            })}
        </div>
    );
}
