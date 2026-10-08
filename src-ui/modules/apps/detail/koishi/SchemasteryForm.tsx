// 按插件自己的 Schemastery schema 画配置表单（Koishi 控制台的插件配置页也是照这份画的）。
// 值是插件在 koishi.yml 里的那张表：没写的键显示默认值，改了才写进去，「恢复默认」就是把键删掉。
// 画不了的节点（函数、自定义类型）给 JSON 原文框，不丢值。
//
// 版式跟其它应用端的配置页一致：交叉里每一段是一个带竖条标题的小节，嵌套对象、列表是带边框的卡片。
// 本文件只留表单入口；纯逻辑在 schemasteryModel.ts，各类型控件在 schemasteryFields/。

import {
    isHidden,
    isObjectLike,
    objectSections,
    type SNode,
} from '../../../../core/domain/apps/koishiSchema';
import { asObj, type Obj } from './schemasteryModel';
import { SchemaField } from './schemasteryFields/SchemaField';
import { SchemaObject } from './schemasteryFields/SchemaObject';
import { InlineUnion } from './schemasteryFields/InlineUnion';

export { SchemaField } from './schemasteryFields/SchemaField';
export { SchemaObject } from './schemasteryFields/SchemaObject';

/** 整个插件（或全局设置）的表单入口；sections = 顶层按段出小节标题 */
export function SchemasteryForm({
    schema,
    value,
    onChange,
    disabled,
    sections = true,
}: {
    schema: SNode;
    value: Obj;
    onChange: (next: Obj) => void;
    disabled?: boolean;
    sections?: boolean;
}) {
    if (schema.type === 'union') {
        return (
            <div className="grid gap-x-5 gap-y-4 sm:grid-cols-2">
                <InlineUnion node={schema} value={value} onChange={onChange} disabled={disabled} />
            </div>
        );
    }
    if (!isObjectLike(schema)) {
        return (
            <div className="grid gap-x-5 gap-y-4 sm:grid-cols-2">
                <SchemaField
                    name="值"
                    node={schema}
                    value={value}
                    onChange={(v) => onChange(asObj(v))}
                    disabled={disabled}
                />
            </div>
        );
    }
    const any = objectSections(schema).some((s) =>
        s.fields.some((f) => f.key === '' || !isHidden(f.node)),
    );
    if (!any) {
        return (
            <p className="py-2 text-[13px] text-text-tertiary">这个插件没有可配置的项，开关就行</p>
        );
    }
    return (
        <SchemaObject
            node={schema}
            value={value}
            onChange={onChange}
            disabled={disabled}
            sections={sections}
        />
    );
}
