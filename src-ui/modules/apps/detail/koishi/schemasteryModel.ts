// Schemastery 表单的纯逻辑层：配置对象的键读写（undefined = 删键回默认）和段的版式规则
// （标签联合按当前值展开、同名段合并、只含一个对象的段摊开成小节）。
// 和 JSX 分开是因为这些规则决定「哪些键写进 koishi.yml、段怎么排」，改版式时得能单独对账。

import {
    describe,
    isHidden,
    isObjectLike,
    objectSections,
    taggedBranch,
    unionShape,
    type SNode,
} from '../../../../core/domain/apps/koishiSchema';
import { splitDescription } from './schemaLabel';

export type Obj = Record<string, unknown>;

export const asObj = (v: unknown): Obj =>
    v && typeof v === 'object' && !Array.isArray(v) ? (v as Obj) : {};

/** 改一个键；undefined = 删键（回到默认） */
export function withKey(obj: Obj, key: string, next: unknown): Obj {
    const out = { ...obj };
    if (next === undefined) delete out[key];
    else out[key] = next;
    return out;
}

export const WIDE = 'sm:col-span-2';

export interface FieldProps {
    name: string;
    node: SNode;
    /** 配置里写着的值；undefined = 没写，按默认显示 */
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
    /** 顶层字段才给「恢复默认」；数组 / 字典里的项没有默认一说 */
    resettable?: boolean;
}

/**
 * 交叉里的一段：没标题但只有一个对象字段的（Koishi 全局设置里的 i18n / delay / request 都是这样），
 * 拿那个字段的描述当小节标题、把它的子字段摊开，不再套一层卡片
 */
export function unwrapSection(section: { title: string; fields: { key: string; node: SNode }[] }) {
    const fields = section.fields.filter((f) => !isHidden(f.node));
    const only = fields.length === 1 ? fields[0] : undefined;
    if (only && only.key && isObjectLike(only.node)) {
        const { title } = splitDescription(describe(only.node));
        return { title: section.title || title || only.key, fields, inner: only };
    }
    return { title: section.title, fields, inner: undefined };
}

/** 分支里去掉判别键那一项（已经有下拉选了） */
export function stripKey(node: SNode, key: string): SNode {
    if (node.type === 'object' && node.dict && key in node.dict) {
        const dict = { ...node.dict };
        delete dict[key];
        return { ...node, dict };
    }
    if (node.type === 'intersect')
        return { ...node, list: (node.list ?? []).map((n) => stripKey(n, key)) };
    return node;
}

export type Section = { title: string; fields: { key: string; node: SNode }[] };

/**
 * 顶层交叉里的标签联合，判别键在别的段里已经是一个下拉（OneBot 适配器先选 protocol，再按它拼连接设置），
 * 就按当前值直接展开成选中那一支自己的几段，不再多给一个「连接方式」下拉，段标题也用分支自己的
 */
export function expandUnions(sections: Section[], value: Obj): Section[] {
    const keys = new Set(sections.flatMap((s) => s.fields.map((f) => f.key)).filter(Boolean));
    return sections.flatMap((s) => {
        const only = s.fields.length === 1 && s.fields[0].key === '' ? s.fields[0].node : undefined;
        const shape = only ? unionShape(only) : undefined;
        if (!shape || shape.kind !== 'tagged' || !keys.has(shape.key)) return [s];
        const branch = shape.branches[taggedBranch(shape, value)];
        if (!branch) return [];
        return objectSections(stripKey(branch.node, shape.key)).map((b) => ({
            ...b,
            title: b.title || s.title,
        }));
    });
}

/** 同名的段并成一段（OneBot 的 ws 分支里「连接设置」拆在两个对象里） */
export function mergeSections(sections: Section[]): Section[] {
    const out: Section[] = [];
    for (const s of sections) {
        const same = s.title ? out.find((o) => o.title === s.title) : undefined;
        if (same) same.fields = [...same.fields, ...s.fields];
        else out.push({ ...s, fields: [...s.fields] });
    }
    return out;
}
