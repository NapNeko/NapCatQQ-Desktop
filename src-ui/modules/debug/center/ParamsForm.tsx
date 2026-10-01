// 参数表单：按接口的参数 schema 一行一个字段，控件按字段角色选（群号 → 群选择器，布尔 → 开关……）。
//
// 表单没有自己的数据：读的是 JSON 文本解析出来的对象，改一个字段就对文本做一次最小修改（setParam），
// 所以表单和 JSON 视图永远是同一份。写回时从 store 里取最新文本再改，连着快速改两个字段也不会互相覆盖。

import { memo, useState, type ReactNode } from 'react';
import { Braces } from 'lucide-react';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { setParam } from '../../../core/domain/debug/paramsText';
import type { FormField, FormModel } from '../../../core/domain/debug/schemaForm';
import type { ParamIssue } from '../../../core/domain/debug/validate';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { cn } from '../../../shared/utils/cn';
import { valueText } from './viewHelpers';
import { FieldHintContext, type FieldHint, type FieldProps } from './fields/fieldKit';
import { TextField } from './fields/TextField';
import { NumberField } from './fields/NumberField';
import { BooleanField } from './fields/BooleanField';
import { EnumField } from './fields/EnumField';
import { ArrayField } from './fields/ArrayField';
import { JsonField } from './fields/JsonField';
import { GroupPicker } from './fields/GroupPicker';
import { FriendPicker } from './fields/FriendPicker';
import { MemberPicker } from './fields/MemberPicker';
import { MessageIdPicker } from './fields/MessageIdPicker';
import { MessageField } from './fields/MessageField';
import { FileField } from './fields/FileField';
import { TimestampField } from './fields/TimestampField';

/** 某个字段对应的输入框 id；「去改这个参数」按它聚焦 */
export function fieldInputId(tabId: string, name: string): string {
    return `debug-param-${tabId}-${name.replace(/[^\w-]/g, '_')}`;
}

/** 校验路径 → 顶层参数名：`message[0].type` → `message` */
export function issueRoot(path: string): string {
    const m = /^[^.[]+/.exec(path);
    return m ? m[0] : path;
}

const KIND_LABEL: Partial<Record<FormField['kind'], string>> = {
    group: '群号',
    friend: 'QQ 号',
    member: '群成员',
    message_id: '消息 ID',
    message: '消息',
    file: '文件',
    face: '表情 ID',
    timestamp: '时间戳',
};

function typeChip(field: FormField): string {
    const role = KIND_LABEL[field.kind];
    if (role) return role;
    if (field.kind === 'array') return field.itemKind === 'number' ? '数字列表' : '文本列表';
    if (field.kind === 'enum') return '枚举';
    if (field.kind === 'json') return 'JSON';
    if (field.kind === 'boolean') return '布尔';
    return field.valueType === 'any' ? '任意' : field.valueType;
}

export interface ParamsFormProps {
    tabId: string;
    model: FormModel;
    values: Record<string, unknown>;
    issues: readonly ParamIssue[];
    target: DebugTarget | null;
    onSubmit: () => void;
    /** 「其它参数」里点「在 JSON 里编辑」 */
    onEditInJson: (key: string) => void;
}

function writeParam(tabId: string, name: string, value: unknown): void {
    const tab = debugWorkspaceStore.getSnapshot().ws.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const next = setParam(tab.params_text, name, value);
    if (next !== tab.params_text) debugWorkspaceStore.setParamsText(tabId, next);
}

export const ParamsForm = memo(function ParamsForm({
    tabId,
    model,
    values,
    issues,
    target,
    onSubmit,
    onEditInJson,
}: ParamsFormProps) {
    const byField = new Map<string, string[]>();
    for (const issue of issues) {
        const root = issueRoot(issue.path);
        const list = byField.get(root) ?? [];
        const sub = issue.path.slice(root.length);
        list.push(sub ? `${sub.replace(/^\./, '')}：${issue.message}` : issue.message);
        byField.set(root, list);
    }
    const known = new Set(model.fields.map((f) => f.name));
    const extras = Object.keys(values).filter((k) => !known.has(k));

    return (
        <div className="@container flex flex-col gap-4 px-3 py-4">
            {model.fields.length === 0 && (
                <p className="rounded-sm bg-inset px-3 py-2.5 text-xs text-text-secondary">
                    这个接口不需要参数，直接发送就行。
                </p>
            )}
            {model.fields.map((field) => (
                <FieldRow
                    key={field.name}
                    tabId={tabId}
                    field={field}
                    value={values[field.name]}
                    groupId={values.group_id}
                    issues={byField.get(field.name)}
                    target={target}
                    onSubmit={onSubmit}
                />
            ))}
            {extras.length > 0 && (
                <section aria-label="其它参数" className="mt-1 border-t border-border-subtle/70 pt-3">
                    <p className="mb-1.5 text-2xs font-medium text-text-tertiary">
                        其它参数<span className="ml-1 font-normal">（接口说明里没有，照样会发出去）</span>
                    </p>
                    <div className="flex flex-wrap gap-1.5">
                        {extras.map((key) => (
                            <span
                                key={key}
                                className="inline-flex max-w-full items-center gap-1.5 rounded-sm border border-border-subtle bg-inset py-0.5 pl-2 pr-1 text-xs"
                            >
                                <span className="font-mono text-text">{key}</span>
                                <span className="max-w-[12rem] truncate font-mono text-text-tertiary" title={valueText(values[key])}>
                                    {valueText(values[key]) || '""'}
                                </span>
                                <button
                                    type="button"
                                    onClick={() => onEditInJson(key)}
                                    className="inline-flex items-center gap-0.5 rounded-xs px-1 py-0.5 text-2xs text-brand hover:bg-brand-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                >
                                    <Braces size={11} aria-hidden />
                                    在 JSON 里编辑
                                </button>
                                {byField.get(key) && <span className="text-2xs text-danger">{byField.get(key)!.join('；')}</span>}
                            </span>
                        ))}
                    </div>
                </section>
            )}
        </div>
    );
});

interface FieldRowProps {
    tabId: string;
    field: FormField;
    value: unknown;
    groupId: unknown;
    issues: string[] | undefined;
    target: DebugTarget | null;
    onSubmit: () => void;
}

// 每行单独 memo：敲一个字段时别的字段（尤其是带选择器、带 JSON 编辑器的）不跟着重渲。
// value 是解析出来的新对象，按 JSON 文本比较才稳定
const FieldRow = memo(
    function FieldRow({ tabId, field, value, groupId, issues, target, onSubmit }: FieldRowProps) {
        const inputId = fieldInputId(tabId, field.name);
        const descId = `${inputId}-desc`;
        const invalid = !!issues && issues.length > 0;
        const [hint, setHint] = useState<FieldHint | null>(null);
        const props: FieldProps = {
            field,
            value,
            onChange: (v) => writeParam(tabId, field.name, v),
            invalid,
            inputId,
            describedBy: descId,
            onSubmit,
        };

        let control: ReactNode;
        switch (field.kind) {
            case 'number':
                control = <NumberField {...props} />;
                break;
            case 'boolean':
                control = <BooleanField {...props} />;
                break;
            case 'enum':
                control = <EnumField {...props} />;
                break;
            case 'array':
                control = <ArrayField {...props} />;
                break;
            case 'json':
                control = <JsonField {...props} ariaLabel={`${field.name} 的 JSON`} />;
                break;
            case 'group':
                control = <GroupPicker {...props} target={target} />;
                break;
            case 'friend':
                control = <FriendPicker {...props} target={target} />;
                break;
            case 'member':
                control = <MemberPicker {...props} target={target} groupId={groupId} />;
                break;
            case 'message_id':
                control = <MessageIdPicker {...props} target={target} />;
                break;
            case 'message':
                control = <MessageField {...props} />;
                break;
            case 'file':
                control = <FileField {...props} />;
                break;
            case 'timestamp':
                control = <TimestampField {...props} />;
                break;
            case 'face':
                control = <TextField {...props} mono placeholder="表情 id，数字" />;
                break;
            default:
                control = <TextField {...props} />;
        }

        const role = KIND_LABEL[field.kind];
        const chip = typeChip(field);
        // NapCat 的说明常常就是「群号」「消息 ID」，和旁边的类型标签一个字不差，再写一遍只是占地方
        const description = field.description && field.description.trim() !== chip ? field.description : undefined;
        // 报错、控件的临时提示、说明并成一行：一个字段底下不再叠两三行小字
        const meta: ReactNode[] = [];
        if (invalid) meta.push(<span key="issue" className="text-danger">{issues!.join('；')}</span>);
        if (hint) {
            meta.push(
                <span key="hint" className={hint.tone === 'warning' ? 'text-warning' : 'text-text-tertiary'}>
                    {hint.text}
                </span>,
            );
        }
        if (description) meta.push(<span key="desc" className="text-text-tertiary">{description}</span>);
        return (
            <div
                data-param={field.name}
                className="grid gap-x-3 gap-y-1 @min-[460px]:grid-cols-[minmax(96px,28%)_minmax(0,1fr)]"
            >
                <label htmlFor={inputId} className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5 @min-[460px]:pt-2.5">
                    <span className="break-all font-mono text-[12.5px] font-medium text-text">
                        {field.name}
                        {field.required && (
                            <span className="ml-0.5 text-danger" aria-label="必填">
                                *
                            </span>
                        )}
                    </span>
                    <span
                        className={cn(
                            'rounded-xs px-1 py-px text-[10px] leading-tight',
                            role ? 'bg-brand-soft text-brand' : 'bg-inset text-text-tertiary',
                        )}
                        title={role ? `认作「${role}」，给了对应的输入方式` : undefined}
                    >
                        {chip}
                    </span>
                </label>
                <div className="min-w-0">
                    <FieldHintContext.Provider value={setHint}>{control}</FieldHintContext.Provider>
                    {meta.length > 0 && (
                        // 不用 role="alert"：敲字时问题一会儿出现一会儿消失，每次都打断朗读；它在 aria-describedby 里，聚焦字段时会读到
                        <p id={descId} className="mt-1.5 line-clamp-2 text-2xs leading-snug" title={description}>
                            {meta.map((part, i) => (
                                <span key={i}>
                                    {i > 0 && (
                                        <span aria-hidden className="mx-1.5 text-text-disabled">
                                            ·
                                        </span>
                                    )}
                                    {part}
                                </span>
                            ))}
                        </p>
                    )}
                </div>
            </div>
        );
    },
    (a, b) =>
        a.tabId === b.tabId &&
        a.field === b.field &&
        a.target === b.target &&
        a.onSubmit === b.onSubmit &&
        a.issues?.join('\n') === b.issues?.join('\n') &&
        valueKey(a.value) === valueKey(b.value) &&
        (a.field.kind !== 'member' || valueKey(a.groupId) === valueKey(b.groupId)),
);

function valueKey(v: unknown): string {
    if (v === undefined) return '\u0000undefined';
    try {
        return JSON.stringify(v) ?? '';
    } catch {
        return String(v);
    }
}
