// 照界面 schema 渲染一节配置：简单字段自动出（开关 / 数字 / 文本 / 下拉 / 字符串列表 / 键值表），
// 对象列表出成一条条卡片（每条按条目的 schema 渲染，可增删），嵌套小节递归成子分组。
// 值始终是整份（整个 bot_config 或 model_config），改一处用 setIn 换出新的整份交回去。

import { Plus, Trash2 } from 'lucide-react';
import {
    Button,
    NumberField,
    Select,
    StringListField,
    Switch,
    TextAreaField,
    TextField,
} from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import {
    fieldOf,
    getIn,
    newItemFor,
    setIn,
    type UiField,
    type UiNode,
} from '../../../../core/domain/apps/maibotSchema';
import type { MaiBotChatSession } from '../../../../core/ipc/types';
import { LineListField, StringMapEditor } from './listEditors';
import { ChatTargetPicker } from './maibotProbes';

// 一行一条编辑的字符串列表：内容里本身有空格逗号（正则、句子、命令行参数），按分隔符拆会拆坏。
// 其余的（账号、库名、token、监听地址）是一个个短 id，用标签输入
const LINE_LISTS = new Set([
    'personality.multiple_reply_style',
    'message_receive.ban_words',
    'message_receive.ban_msgs_regex',
    'keyword_reaction.keyword_rules.keywords',
    'keyword_reaction.keyword_rules.regex',
    'keyword_reaction.regex_rules.keywords',
    'keyword_reaction.regex_rules.regex',
    'mcp.servers.args',
    'plugin_runtime.render.launch_args',
]);

// 上游只给了说明、没给标签的常见取值。日志级别、检索模式这些本来就是术语，原样显示
const OPTION_LABELS: Readonly<Record<string, string>> = {
    group: '群聊',
    private: '私聊',
    whitelist: '白名单',
    blacklist: '黑名单',
    text: '纯文本',
    multimodal: '多模态',
    auto: '自动',
    compress: '压缩',
    discard: '丢弃',
    development: '开发',
    production: '正式',
};

export interface WidgetProps {
    field: UiField;
    value: unknown;
    set: (next: unknown) => void;
    error?: string;
    disabled?: boolean;
}

export interface SchemaCtx {
    /** 整份配置的值 */
    value: unknown;
    onChange: (next: unknown) => void;
    errors: Record<string, string>;
    /** 错误路径前缀，和后端一致：bot / models */
    prefix: string;
    disabled?: boolean;
    showAdvanced: boolean;
    /** 不在这里出的字段：别的页已经管着，每个设置只在一处改 */
    skip?: ReadonlySet<string>;
    /** 上游没标高级、但在桌面端只是兜底用的字段，收进高级里 */
    advanced?: ReadonlySet<string>;
    /** 按字段换控件（如模型的服务商从已有的里选） */
    widgets?: Readonly<Record<string, (p: WidgetProps) => React.ReactNode>>;
    /** 麦麦见过的聊天；给了的话，按聊天配的列表多一个「从聊过的里选」 */
    chatTargets?: readonly MaiBotChatSession[];
}

/** 条目是「平台 + 聊天 ID + 群 / 私聊」的列表（按聊天的规则、学习名单、共享组成员），返回类型字段名 */
function chatTargetTypeField(item: UiNode): string | null {
    const names = new Set(item.fields.map((f) => f.name));
    if (!names.has('platform') || !names.has('item_id')) return null;
    return names.has('rule_type') ? 'rule_type' : names.has('type') ? 'type' : null;
}

/** 在两栏网格里占满一整行；自定义控件要宽就套它 */
export const WIDE = 'sm:col-span-2';

/**
 * 字段的形状键：点分路径去掉列表下标，如 mcp.servers.name。
 * skip / advanced / widgets 都按它认，列表里每一条同一个字段用同一条规则。
 */
export function shapeKey(path: readonly string[], name: string): string {
    return [...path, name].filter((s) => !/^\d+$/.test(s)).join('.');
}

function errorOf(ctx: SchemaCtx, path: readonly string[]): string | undefined {
    return ctx.errors[`${ctx.prefix}/${path.join('/')}`];
}

function hidden(ctx: SchemaCtx, path: readonly string[], field: UiField): boolean {
    const key = shapeKey(path, field.name);
    if (field.hidden || ctx.skip?.has(key)) return true;
    return !ctx.showAdvanced && (!!field.advanced || !!ctx.advanced?.has(key));
}

/** 这一节在当前开关下还剩不剩要出的字段；全收起了就连小节标题也不出 */
export function anyVisible(ctx: SchemaCtx, path: readonly string[], node: UiNode): boolean {
    return node.fields.some((f) => {
        if (hidden(ctx, path, f)) return false;
        const sub = node.nested?.[f.name];
        if (!sub || f.type === 'array' || ctx.widgets?.[shapeKey(path, f.name)]) return true;
        if (sub.uiAdvanced && !ctx.showAdvanced) return false;
        return anyVisible(ctx, [...path, f.name], sub);
    });
}

/**
 * 这一节的校验错误里有没有落在高级字段上的（字段本身、它所在的列表或子分组标了高级都算）。
 * 错误路径和后端一致：前缀 / 小节路径 / 字段，列表条目带下标
 */
export function advancedHasError(ctx: SchemaCtx, path: readonly string[], node: UiNode): boolean {
    const base = `${ctx.prefix}/${path.join('/')}/`;
    return Object.keys(ctx.errors).some((key) => {
        if (!key.startsWith(base)) return false;
        let cur: UiNode | undefined = node;
        const at = [...path];
        for (const seg of key.slice(base.length).split('/')) {
            if (!/^\d+$/.test(seg)) {
                const f = fieldOf(cur, seg);
                const key = shapeKey(at, seg);
                if (!f || f.hidden || ctx.skip?.has(key)) return false;
                if (f.advanced || ctx.advanced?.has(key)) return true;
                cur = cur?.nested?.[seg];
                if (cur?.uiAdvanced) return true;
            }
            at.push(seg);
        }
        return false;
    });
}

/**
 * 一节里有没有收起来的高级字段（决定小节标题要不要给「高级选项」）。
 * 这页不出的字段不算，不然点开什么也不多
 */
export function hasAdvanced(ctx: SchemaCtx, path: readonly string[], node: UiNode | undefined): boolean {
    if (!node) return false;
    return node.fields.some((f) => {
        const key = shapeKey(path, f.name);
        if (f.hidden || ctx.skip?.has(key)) return false;
        if (f.advanced || ctx.advanced?.has(key)) return true;
        const sub = node.nested?.[f.name];
        if (!sub || ctx.widgets?.[key]) return false;
        return !!sub.uiAdvanced || hasAdvanced(ctx, [...path, f.name], sub);
    });
}

const SimpleField: React.FC<{ ctx: SchemaCtx; path: readonly string[]; field: UiField }> = ({ ctx, path, field }) => {
    const p = [...path, field.name];
    const value = getIn(ctx.value, p);
    const error = errorOf(ctx, p);
    const set = (next: unknown) => ctx.onChange(setIn(ctx.value, p, next));
    const label = field.label ?? field.name;
    const hint = field.description;
    const widget = field['x-widget'];
    const custom = ctx.widgets?.[shapeKey(path, field.name)];
    if (custom) return <>{custom({ field, value, set, error, disabled: ctx.disabled })}</>;

    // 上游有的字符串字段只给了 options 没标 select（如任务的选择策略）
    const type = field.type === 'string' && field.options?.length ? 'select' : field.type;
    switch (type) {
        case 'boolean':
            return (
                <Switch
                    label={label}
                    hint={hint}
                    checked={value === true}
                    disabled={ctx.disabled}
                    onCheckedChange={set}
                />
            );
        case 'integer':
        case 'number':
            return (
                <NumberField
                    label={label}
                    hint={hint}
                    error={error}
                    value={typeof value === 'number' ? value : null}
                    allowFloat={field.type === 'number'}
                    min={field.minValue}
                    max={field.maxValue}
                    step={field.step}
                    disabled={ctx.disabled}
                    onValueChange={(n) => {
                        if (n !== null) set(n);
                    }}
                />
            );
        case 'select': {
            const labels = field['x-option-labels'] ?? {};
            const descs = field['x-option-descriptions'] ?? {};
            const current = typeof value === 'string' ? value : '';
            return (
                <Select
                    label={label}
                    hint={descs[current] ?? hint}
                    error={error}
                    value={current}
                    items={(field.options ?? []).map((o) => ({ value: o, label: labels[o] ?? OPTION_LABELS[o] ?? o }))}
                    disabled={ctx.disabled}
                    onValueChange={set}
                />
            );
        }
        case 'array': {
            const list = Array.isArray(value) ? value.map(String) : [];
            if (LINE_LISTS.has(shapeKey(path, field.name))) {
                const itemErrors = Object.fromEntries(list.map((_, i) => [i, errorOf(ctx, [...p, String(i)])]));
                return (
                    <LineListField
                        className={WIDE}
                        label={label}
                        hint={hint}
                        error={error}
                        itemErrors={itemErrors}
                        value={list}
                        mono={/regex|args/.test(field.name)}
                        disabled={ctx.disabled}
                        onChange={set}
                    />
                );
            }
            return (
                <StringListField
                    className={WIDE}
                    label={label}
                    hint={hint}
                    error={error}
                    value={list}
                    placeholder={field.placeholder}
                    disabled={ctx.disabled}
                    onChange={set}
                />
            );
        }
        default: {
            const text = typeof value === 'string' ? value : '';
            if (widget === 'textarea') {
                return (
                    <TextAreaField
                        className={WIDE}
                        label={label}
                        hint={hint}
                        error={error}
                        value={text}
                        minRows={field['x-textarea-rows'] ?? 3}
                        disabled={ctx.disabled}
                        onValueChange={set}
                    />
                );
            }
            return (
                <TextField
                    label={label}
                    hint={hint}
                    error={error}
                    value={text}
                    type={widget === 'password' ? 'password' : 'text'}
                    placeholder={field.placeholder}
                    disabled={ctx.disabled}
                    onValueChange={set}
                />
            );
        }
    }
};

/** 字符串到字符串的映射（请求头、环境变量、日志库级别）；值是对象的映射这里改不了，指去原始文件 */
const StringMapField: React.FC<{ ctx: SchemaCtx; path: readonly string[]; field: UiField }> = ({ ctx, path, field }) => {
    const p = [...path, field.name];
    const raw = getIn(ctx.value, p);
    const map = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
    const label = field.label ?? field.name;
    if (Object.values(map).some((v) => v !== null && typeof v === 'object')) {
        return (
            <div className={cn('flex flex-col gap-1', WIDE)}>
                <span className="text-xs font-medium text-text-secondary">{label}</span>
                <p className="text-2xs text-text-tertiary">
                    已配 {Object.keys(map).length} 项，结构较深，到「原始文件」页改
                </p>
            </div>
        );
    }
    return (
        <StringMapEditor
            className={WIDE}
            label={label}
            hint={field.description}
            error={errorOf(ctx, p)}
            value={map as Record<string, string>}
            disabled={ctx.disabled}
            onChange={(next) => ctx.onChange(setIn(ctx.value, p, next))}
        />
    );
};

const ObjectListField: React.FC<{ ctx: SchemaCtx; path: readonly string[]; field: UiField; item: UiNode }> = ({
    ctx,
    path,
    field,
    item,
}) => {
    const p = [...path, field.name];
    const raw = getIn(ctx.value, p);
    const items = Array.isArray(raw) ? raw : [];
    const set = (next: unknown[]) => ctx.onChange(setIn(ctx.value, p, next));
    const typeField = chatTargetTypeField(item);
    return (
        <div className={cn('flex flex-col gap-2', WIDE)}>
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                    <span className="text-xs font-medium text-text-secondary">{field.label ?? field.name}</span>
                    {field.description && <p className="text-2xs text-text-tertiary">{field.description}</p>}
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                    {typeField && ctx.chatTargets && (
                        <ChatTargetPicker
                            sessions={ctx.chatTargets}
                            disabled={ctx.disabled}
                            onPick={(s) =>
                                set([
                                    ...items,
                                    {
                                        ...newItemFor(item),
                                        platform: s.platform,
                                        item_id: s.target_id,
                                        [typeField]: s.chat_type === 'private' ? 'private' : 'group',
                                    },
                                ])
                            }
                        />
                    )}
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={ctx.disabled}
                        onClick={() => set([...items, newItemFor(item)])}
                    >
                        <ActionMotionIcon icon={Plus} size={13} />
                        加一条
                    </Button>
                </div>
            </div>
            {items.length === 0 ? (
                <p className="text-2xs text-text-tertiary">还没有</p>
            ) : (
                items.map((_, i) => (
                    <div key={i} className="rounded-md border border-border-subtle bg-inset/30 p-3">
                        <div className="mb-2 flex items-center justify-between">
                            <span className="text-2xs text-text-tertiary">第 {i + 1} 条</span>
                            <button
                                type="button"
                                aria-label="删掉这一条"
                                disabled={ctx.disabled}
                                onClick={() => set(items.filter((_, j) => j !== i))}
                                className="inline-flex h-7 w-7 items-center justify-center rounded-xs text-danger hover:bg-danger-soft disabled:opacity-40"
                            >
                                <Trash2 size={14} strokeWidth={2.2} />
                            </button>
                        </div>
                        <FieldGrid ctx={ctx} path={[...p, String(i)]} node={item} />
                    </div>
                ))
            )}
        </div>
    );
};

/** 一节里的字段；嵌套的对象小节收在最后，按层级缩进出小标题 */
export const FieldGrid: React.FC<{ ctx: SchemaCtx; path: readonly string[]; node: UiNode }> = ({ ctx, path, node }) => {
    const cells: React.ReactNode[] = [];
    const subs: React.ReactNode[] = [];
    for (const field of node.fields) {
        if (hidden(ctx, path, field)) continue;
        const sub = node.nested?.[field.name];
        if (ctx.widgets?.[shapeKey(path, field.name)]) {
            cells.push(<SimpleField key={field.name} ctx={ctx} path={path} field={field} />);
        } else if (sub && field.type === 'array') {
            cells.push(<ObjectListField key={field.name} ctx={ctx} path={path} field={field} item={sub} />);
        } else if (sub) {
            if (sub.uiAdvanced && !ctx.showAdvanced) continue;
            subs.push(
                <SubSection key={field.name} title={sub.uiLabel ?? field.label ?? field.name} hint={field.description}>
                    <FieldGrid ctx={ctx} path={[...path, field.name]} node={sub} />
                </SubSection>,
            );
        } else if (field.type === 'object') {
            cells.push(<StringMapField key={field.name} ctx={ctx} path={path} field={field} />);
        } else {
            cells.push(<SimpleField key={field.name} ctx={ctx} path={path} field={field} />);
        }
    }
    if (!cells.length && !subs.length) return null;
    return (
        <div className="flex flex-col gap-5">
            {cells.length > 0 && <div className="grid grid-cols-1 gap-x-5 gap-y-4 sm:grid-cols-2">{cells}</div>}
            {subs}
        </div>
    );
};

const SubSection: React.FC<{ title: string; hint?: string; children: React.ReactNode }> = ({ title, hint, children }) => (
    <div className="flex flex-col gap-3 border-l-2 border-border-subtle pl-4">
        <div>
            <h4 className="text-sm font-medium text-text">{title}</h4>
            {hint && <p className="text-2xs text-text-tertiary">{hint}</p>}
        </div>
        {children}
    </div>
);
