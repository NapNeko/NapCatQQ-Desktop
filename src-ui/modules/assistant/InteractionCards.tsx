// 需要桌面用户回应的操作审批与追问。

import { useState } from 'react';
import { HelpCircle, Loader2, ShieldCheck } from 'lucide-react';
import type {
    AssistantPermissionRequest,
    AssistantQuestion,
    AssistantQuestionAnswer,
} from '../../core/domain/assistant/types';
import { Badge, Button, Select, TextField } from '../../shared/ui';

interface PermissionProps {
    request: AssistantPermissionRequest;
    pending: boolean;
    onRespond: (optionId: string | null) => void;
}

export function PermissionCard({ request, pending, onRespond }: PermissionProps) {
    return (
        <section
            className="space-y-3 rounded-lg border border-warning/30 bg-warning-soft/50 p-4"
            aria-label="操作审批"
        >
            <div className="flex flex-wrap items-center gap-2 text-sm font-semibold text-text">
                <ShieldCheck size={16} className="text-warning" />
                <h3>{request.title}</h3>
                <Badge tone="warning">
                    {request.source === 'ncdOperation' ? 'NCD 操作审批' : 'Agent 权限请求'}
                </Badge>
            </div>
            {request.details && (
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words font-sans text-xs leading-5 text-text-secondary">
                    {request.details}
                </pre>
            )}
            <div className="flex flex-wrap gap-2">
                {request.options.map((option) => (
                    <Button
                        key={option.id}
                        size="sm"
                        disabled={pending}
                        variant={option.kind === 'allow_once' ? 'primary' : 'secondary'}
                        onClick={() => onRespond(option.id)}
                    >
                        {pending && <Loader2 size={12} className="animate-spin" />}
                        {option.label}
                    </Button>
                ))}
                {!request.options.some((option) => option.kind.startsWith('reject')) && (
                    <Button size="sm" disabled={pending} onClick={() => onRespond(null)}>
                        拒绝
                    </Button>
                )}
            </div>
        </section>
    );
}

interface QuestionProps {
    question: AssistantQuestion;
    pending: boolean;
    draft: string;
    onDraft: (value: string) => void;
    onRespond: (answer: AssistantQuestionAnswer) => void;
}

function parseDraft(draft: string): Record<string, string> {
    try {
        return JSON.parse(draft || '{}') as Record<string, string>;
    } catch {
        return {};
    }
}

export function QuestionCard({ question, pending, draft, onDraft, onRespond }: QuestionProps) {
    const values = parseDraft(draft);
    const [submitted, setSubmitted] = useState(false);
    const change = (name: string, value: string) =>
        onDraft(JSON.stringify({ ...values, [name]: value }));
    const valid = question.fields.every((field) => !field.required || values[field.name]?.trim());
    return (
        <form
            className="space-y-3 rounded-lg border border-brand/20 bg-brand-soft/40 p-4"
            aria-label="补充信息"
            onSubmit={(event) => {
                event.preventDefault();
                setSubmitted(true);
                if (valid) onRespond({ cancelled: false, values });
            }}
        >
            <div className="flex items-center gap-2 text-sm font-semibold text-text">
                <HelpCircle size={16} className="text-brand" />
                <h3>{question.title}</h3>
            </div>
            {question.fields.map((field) => {
                const choices = field.options.length ? field.options : ['true', 'false'];
                // shared Select 把空字符串视为非受控；占位项避免首次作答时切换模式。
                let unselected = '__ncd_unselected__';
                while (choices.includes(unselected)) unselected += '_';
                return (
                    <div key={field.name} className="space-y-1.5">
                        {field.options.length > 0 || field.kind === 'boolean' ? (
                            <Select
                                label={field.title}
                                required={field.required}
                                disabled={pending}
                                value={values[field.name] || unselected}
                                placeholder="请选择"
                                items={[
                                    { value: unselected, label: '请选择', disabled: true },
                                    ...choices.map((value) => ({
                                        value,
                                        label:
                                            field.kind === 'boolean'
                                                ? value === 'true'
                                                    ? '是'
                                                    : '否'
                                                : value,
                                    })),
                                ]}
                                onValueChange={(value) => {
                                    if (value !== unselected) change(field.name, value);
                                }}
                                error={
                                    submitted && field.required && !values[field.name]?.trim()
                                        ? '请填写此项'
                                        : undefined
                                }
                            />
                        ) : (
                            <TextField
                                label={field.title}
                                required={field.required}
                                disabled={pending}
                                type={
                                    field.kind === 'number' || field.kind === 'integer'
                                        ? 'number'
                                        : 'text'
                                }
                                step={
                                    field.kind === 'number'
                                        ? 'any'
                                        : field.kind === 'integer'
                                          ? 1
                                          : undefined
                                }
                                value={values[field.name] ?? ''}
                                onValueChange={(value) => change(field.name, value)}
                                error={
                                    submitted && field.required && !values[field.name]?.trim()
                                        ? '请填写此项'
                                        : undefined
                                }
                            />
                        )}
                        {field.description && (
                            <p className="text-xs leading-5 text-text-secondary">
                                {field.description}
                            </p>
                        )}
                    </div>
                );
            })}
            <div className="flex gap-2">
                <Button size="sm" type="submit" variant="primary" disabled={pending}>
                    {pending && <Loader2 size={12} className="animate-spin" />}提交
                </Button>
                <Button
                    size="sm"
                    disabled={pending}
                    onClick={() => onRespond({ cancelled: true, values: {} })}
                >
                    跳过
                </Button>
            </div>
        </form>
    );
}
