// 按插件自己的 Schemastery schema 画配置表单（Koishi 控制台的插件配置页也是照这份画的）。
// 值是插件在 koishi.yml 里的那张表：没写的键显示默认值，改了才写进去，「恢复默认」就是把键删掉。
// 画不了的节点（函数、自定义类型）给 JSON 原文框，不丢值。
//
// 版式跟其它应用端的配置页一致：交叉里每一段是一个带竖条标题的小节，嵌套对象、列表是带边框的卡片。

import { useState } from 'react';
import { ChevronRight, Plus, Trash2 } from 'lucide-react';
import {
    Button,
    FormSection,
    NumberField,
    Select,
    StringListField,
    Switch,
    TextAreaField,
    TextField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    blankOf,
    describe,
    effectiveValue,
    isHidden,
    isObjectLike,
    mixedBranch,
    objectFields,
    objectSections,
    renderable,
    taggedBranch,
    unionShape,
    type SNode,
} from '../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText, splitDescription } from './schemaLabel';

type Obj = Record<string, unknown>;

const asObj = (v: unknown): Obj =>
    v && typeof v === 'object' && !Array.isArray(v) ? (v as Obj) : {};

/** 改一个键；undefined = 删键（回到默认） */
function withKey(obj: Obj, key: string, next: unknown): Obj {
    const out = { ...obj };
    if (next === undefined) delete out[key];
    else out[key] = next;
    return out;
}

const WIDE = 'sm:col-span-2';

interface FieldProps {
    name: string;
    node: SNode;
    /** 配置里写着的值；undefined = 没写，按默认显示 */
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
    /** 顶层字段才给「恢复默认」；数组 / 字典里的项没有默认一说 */
    resettable?: boolean;
}

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

/** 嵌套对象 / 列表 / 字典：带边框的卡片，标题行可折叠；schema 标了 collapse 的默认收起 */
function NestedCard({
    name,
    node,
    onReset,
    disabled,
    actions,
    count,
    children,
}: {
    name: string;
    node: SNode;
    onReset?: () => void;
    disabled?: boolean;
    actions?: React.ReactNode;
    count?: number;
    children: React.ReactNode;
}) {
    const [open, setOpen] = useState(!node.meta.collapse);
    const { hint } = fieldText(name, node);
    return (
        <div
            className={cn(
                'flex min-w-0 flex-col rounded-lg border border-border-subtle bg-surface/60',
                WIDE,
            )}
        >
            <div className="flex min-h-[44px] items-center gap-2 px-3.5 py-2">
                <button
                    type="button"
                    aria-expanded={open}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left text-[13px] font-medium text-text"
                    onClick={() => setOpen((v) => !v)}
                >
                    <ChevronRight
                        size={14}
                        className={cn(
                            'shrink-0 text-text-tertiary transition-transform duration-200',
                            open && 'rotate-90',
                        )}
                    />
                    <FieldLabel name={name} node={node} />
                    {count !== undefined && (
                        <span className="rounded-pill bg-inset px-1.5 text-2xs font-normal tabular-nums text-text-tertiary">
                            {count}
                        </span>
                    )}
                </button>
                <div className="flex shrink-0 items-center gap-1">
                    {actions}
                    {onReset && (
                        <Button size="sm" variant="ghost" disabled={disabled} onClick={onReset}>
                            恢复默认
                        </Button>
                    )}
                </div>
            </div>
            {hint && (
                <p className="-mt-1 px-3.5 pb-2 pl-9 text-2xs leading-relaxed text-text-tertiary">
                    {hint}
                </p>
            )}
            {open && <div className="border-t border-border-subtle/70 px-3.5 py-3">{children}</div>}
        </div>
    );
}

/**
 * 交叉里的一段：没标题但只有一个对象字段的（Koishi 全局设置里的 i18n / delay / request 都是这样），
 * 拿那个字段的描述当小节标题、把它的子字段摊开，不再套一层卡片
 */
function unwrapSection(section: { title: string; fields: { key: string; node: SNode }[] }) {
    const fields = section.fields.filter((f) => !isHidden(f.node));
    const only = fields.length === 1 ? fields[0] : undefined;
    if (only && only.key && isObjectLike(only.node)) {
        const { title } = splitDescription(describe(only.node));
        return { title: section.title || title || only.key, fields, inner: only };
    }
    return { title: section.title, fields, inner: undefined };
}

function FieldGrid({
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

/** 交叉里的一段标签联合（如 OneBot 适配器按 protocol 分三种连法）：判别键和分支字段写在同一张表上 */
function InlineUnion({
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

/** 分支里去掉判别键那一项（已经有下拉选了） */
function stripKey(node: SNode, key: string): SNode {
    if (node.type === 'object' && node.dict && key in node.dict) {
        const dict = { ...node.dict };
        delete dict[key];
        return { ...node, dict };
    }
    if (node.type === 'intersect')
        return { ...node, list: (node.list ?? []).map((n) => stripKey(n, key)) };
    return node;
}

type Section = { title: string; fields: { key: string; node: SNode }[] };

/**
 * 顶层交叉里的标签联合，判别键在别的段里已经是一个下拉（OneBot 适配器先选 protocol，再按它拼连接设置），
 * 就按当前值直接展开成选中那一支自己的几段，不再多给一个「连接方式」下拉，段标题也用分支自己的
 */
function expandUnions(sections: Section[], value: Obj): Section[] {
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
function mergeSections(sections: Section[]): Section[] {
    const out: Section[] = [];
    for (const s of sections) {
        const same = s.title ? out.find((o) => o.title === s.title) : undefined;
        if (same) same.fields = [...same.fields, ...s.fields];
        else out.push({ ...s, fields: [...s.fields] });
    }
    return out;
}

function UnionField({
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

function ItemRemove({ onClick, disabled }: { onClick: () => void; disabled?: boolean }) {
    return (
        <button
            type="button"
            aria-label="删除这一项"
            title="删除这一项"
            disabled={disabled}
            onClick={onClick}
            className="mt-6 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-danger-soft hover:text-danger disabled:opacity-40"
        >
            <Trash2 size={14} />
        </button>
    );
}

function ArrayField({
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

/** 字典的键：边打边改会让整行换 key 重挂、丢焦点，所以失焦再改名 */
function DictKeyInput({
    value,
    onCommit,
    disabled,
}: {
    value: string;
    onCommit: (next: string) => void;
    disabled?: boolean;
}) {
    const [draft, setDraft] = useState(value);
    return (
        <TextField
            label="键"
            value={draft}
            disabled={disabled}
            onValueChange={setDraft}
            onBlur={() => {
                const next = draft.trim();
                if (next && next !== value) onCommit(next);
                else setDraft(value);
            }}
        />
    );
}

function DictField({
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
    const obj = asObj(value);
    const entries = Object.entries(obj);
    const inner = node.inner;
    const [draftKey, setDraftKey] = useState('');
    if (!inner) return null;
    const rename = (from: string, to: string) => {
        if (!to || to === from || to in obj) return;
        const next: Obj = {};
        for (const [k, v] of entries) next[k === from ? to : k] = v;
        onChange(next);
    };
    const key = draftKey.trim();
    return (
        <NestedCard
            name={name}
            node={node}
            onReset={canReset ? () => onChange(undefined) : undefined}
            disabled={disabled}
            count={entries.length}
        >
            <ul className="flex flex-col divide-y divide-border-subtle/70">
                {entries.map(([k, v]) => (
                    <li key={k} className="flex items-start gap-3 py-3 first:pt-0">
                        <div className="w-44 shrink-0">
                            <DictKeyInput
                                value={k}
                                disabled={disabled}
                                onCommit={(to) => rename(k, to)}
                            />
                        </div>
                        <div className="grid min-w-0 flex-1 gap-3">
                            {isObjectLike(inner) ? (
                                <SchemaObject
                                    node={inner}
                                    value={asObj(v)}
                                    disabled={disabled}
                                    onChange={(nv) => onChange({ ...obj, [k]: nv })}
                                />
                            ) : (
                                <SchemaField
                                    name="值"
                                    node={inner}
                                    value={v}
                                    resettable={false}
                                    disabled={disabled}
                                    onChange={(nv) => onChange({ ...obj, [k]: nv })}
                                />
                            )}
                        </div>
                        <ItemRemove
                            disabled={disabled}
                            onClick={() => onChange(withKey(obj, k, undefined))}
                        />
                    </li>
                ))}
            </ul>
            <div
                className={cn(
                    'flex items-center gap-2',
                    entries.length > 0 && 'mt-3 border-t border-border-subtle/70 pt-3',
                )}
            >
                <div className="w-44">
                    <TextField
                        value={draftKey}
                        placeholder="新的键"
                        disabled={disabled}
                        onValueChange={setDraftKey}
                    />
                </div>
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={disabled || !key || key in obj}
                    onClick={() => {
                        onChange({ ...obj, [key]: blankOf(inner) });
                        setDraftKey('');
                    }}
                >
                    <Plus size={13} />
                    加一项
                </Button>
            </div>
        </NestedCard>
    );
}

/** 画不了的节点：JSON 原文，失焦时解析，解析不了就不写回 */
function JsonField({
    name,
    node,
    value,
    onChange,
    disabled,
}: {
    name: string;
    node: SNode;
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
}) {
    const [text, setText] = useState(() =>
        value === undefined ? '' : JSON.stringify(value, null, 2),
    );
    const [error, setError] = useState<string | null>(null);
    const { hint } = fieldText(name, node);
    return (
        <TextAreaField
            className={WIDE}
            label={<FieldLabel name={name} node={node} />}
            hint={error ?? `${hint ? `${hint} ` : ''}这一项表单画不了，按 JSON 填；留空用默认值`}
            error={error ?? undefined}
            value={text}
            mono
            minRows={3}
            disabled={disabled}
            onValueChange={setText}
            onBlur={() => {
                if (!text.trim()) {
                    setError(null);
                    onChange(undefined);
                    return;
                }
                try {
                    onChange(JSON.parse(text));
                    setError(null);
                } catch (e) {
                    setError(`不是合法的 JSON：${(e as Error).message}`);
                }
            }}
        />
    );
}

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
