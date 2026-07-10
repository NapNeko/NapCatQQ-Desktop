// Webhook 消息可视化编辑器：中栏字段 + 右栏原始 JSON（实时同步，可预览可编辑）。
// 持久化仍是 body_template 字符串。字段/JSON 共用撤销栈，响应 Ctrl+Z / Ctrl+Y。

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import type { WebhookPresetId } from '../../../core/domain/settings/offline-notify-defaults';
import {
    detectWebhookService,
    fieldsFromPresetBody,
    parseVisualFields,
    serializeVisualFields,
    serviceFieldMeta,
    TEMPLATE_VARS,
    type WebhookServiceKind,
    type WebhookVisualFields,
} from '../../../core/domain/settings/webhook-message-visual';
import { Badge, TextField } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { ThemeJsonEditor } from './ThemeJsonEditor';

interface Props {
    bodyTemplate: string;
    serviceHint?: WebhookPresetId | null;
    onBodyTemplateChange: (next: string) => void;
    className?: string;
}

const HISTORY_LIMIT = 80;

function softEqualJson(a: string, b: string): boolean {
    try {
        return JSON.stringify(JSON.parse(a)) === JSON.stringify(JSON.parse(b));
    } catch {
        return a.trim() === b.trim();
    }
}

function isValidJson(text: string): boolean {
    try {
        JSON.parse(text);
        return true;
    } catch {
        return false;
    }
}

function fieldsForTemplate(
    template: string,
    hint: WebhookPresetId | null,
): { kind: WebhookServiceKind; fields: WebhookVisualFields } {
    const kind = hint ?? detectWebhookService(template);
    if (kind === 'custom') {
        return { kind, fields: fieldsFromPresetBody('serverchan') };
    }
    return {
        kind,
        fields: parseVisualFields(template, kind) ?? fieldsFromPresetBody(kind),
    };
}

export function WebhookMessageBuilder({
    bodyTemplate,
    serviceHint = null,
    onBodyTemplateChange,
    className,
}: Props) {
    const initial = fieldsForTemplate(bodyTemplate, serviceHint);
    const [fields, setFields] = useState<WebhookVisualFields>(initial.fields);
    const [rawDraft, setRawDraft] = useState(bodyTemplate);
    const activeFieldRef = useRef<'title' | 'body' | 'group' | null>(null);

    // 撤销栈存 body_template 快照；受控输入每键写入父级会打断浏览器原生 undo。
    const historyRef = useRef<string[]>([bodyTemplate]);
    const historyIndexRef = useRef(0);
    const lastEmittedRef = useRef(bodyTemplate);
    const applyingHistoryRef = useRef(false);

    const kind: WebhookServiceKind =
        serviceHint ?? detectWebhookService(rawDraft);
    const meta = serviceFieldMeta(kind);
    const isCustom = kind === 'custom';

    const applyTemplateLocal = useCallback(
        (template: string, hint: WebhookPresetId | null = serviceHint) => {
            setRawDraft(template);
            const next = fieldsForTemplate(template, hint);
            if (next.kind !== 'custom') setFields(next.fields);
        },
        [serviceHint],
    );

    const commitTemplate = useCallback(
        (template: string, opts?: { recordHistory?: boolean }) => {
            const recordHistory = opts?.recordHistory !== false;
            if (recordHistory && !applyingHistoryRef.current) {
                const hist = historyRef.current;
                const idx = historyIndexRef.current;
                if (hist[idx] !== template) {
                    const next = hist.slice(0, idx + 1);
                    next.push(template);
                    if (next.length > HISTORY_LIMIT) {
                        next.splice(0, next.length - HISTORY_LIMIT);
                    }
                    historyRef.current = next;
                    historyIndexRef.current = next.length - 1;
                }
            }
            lastEmittedRef.current = template;
            onBodyTemplateChange(template);
        },
        [onBodyTemplateChange],
    );

    // 父级变更（切服务 / 打开通道）：对齐本地并重置撤销栈
    useEffect(() => {
        if (softEqualJson(bodyTemplate, lastEmittedRef.current)) return;
        lastEmittedRef.current = bodyTemplate;
        applyTemplateLocal(bodyTemplate, serviceHint);
        historyRef.current = [bodyTemplate];
        historyIndexRef.current = 0;
    }, [bodyTemplate, serviceHint, applyTemplateLocal]);

    const undo = useCallback(() => {
        if (historyIndexRef.current <= 0) return;
        applyingHistoryRef.current = true;
        historyIndexRef.current -= 1;
        const prev = historyRef.current[historyIndexRef.current] ?? '';
        applyTemplateLocal(prev, serviceHint);
        lastEmittedRef.current = prev;
        onBodyTemplateChange(prev);
        applyingHistoryRef.current = false;
    }, [applyTemplateLocal, onBodyTemplateChange, serviceHint]);

    const redo = useCallback(() => {
        if (historyIndexRef.current >= historyRef.current.length - 1) return;
        applyingHistoryRef.current = true;
        historyIndexRef.current += 1;
        const next = historyRef.current[historyIndexRef.current] ?? '';
        applyTemplateLocal(next, serviceHint);
        lastEmittedRef.current = next;
        onBodyTemplateChange(next);
        applyingHistoryRef.current = false;
    }, [applyTemplateLocal, onBodyTemplateChange, serviceHint]);

    const emitVisual = (
        next: WebhookVisualFields,
        nextKind: WebhookServiceKind = kind,
    ) => {
        if (nextKind === 'custom') return;
        const serialized = serializeVisualFields(nextKind, next);
        setRawDraft(serialized);
        commitTemplate(serialized);
    };

    const patchFields = (patch: Partial<WebhookVisualFields>) => {
        setFields((cur) => {
            const next = { ...cur, ...patch };
            emitVisual(next);
            return next;
        });
    };

    const insertVar = (token: string) => {
        const target =
            activeFieldRef.current ?? (meta.showBody ? 'body' : 'title');
        if (target === 'group' || (target === 'title' && !meta.showTitle)) {
            const key = meta.showBody ? 'body' : 'title';
            patchFields({ [key]: `${fields[key]}${token}` });
            return;
        }
        patchFields({ [target]: `${fields[target]}${token}` });
    };

    const onRawCommit = (raw: string) => {
        setRawDraft(raw);
        commitTemplate(raw);
        const detected = detectWebhookService(raw);
        if (detected !== 'custom') {
            const parsed = parseVisualFields(raw, detected);
            if (parsed) setFields(parsed);
        }
    };

    const onKeyDown = (e: KeyboardEvent) => {
        const mod = e.ctrlKey || e.metaKey;
        if (!mod) return;
        const key = e.key.toLowerCase();
        if (key === 'z' && !e.shiftKey) {
            e.preventDefault();
            e.stopPropagation();
            undo();
            return;
        }
        if (key === 'y' || (key === 'z' && e.shiftKey)) {
            e.preventDefault();
            e.stopPropagation();
            redo();
        }
    };

    const jsonOk = useMemo(() => isValidJson(rawDraft), [rawDraft]);

    return (
        <div
            className={cn(
                'grid min-h-0 min-w-0 flex-1 grid-cols-1 gap-3 lg:grid-cols-2 lg:gap-3',
                className,
            )}
            onKeyDown={onKeyDown}
        >
            <section className="flex min-h-0 min-w-0 flex-col gap-2.5 overflow-hidden rounded-sm border border-border-subtle bg-inset/20 p-3">
                <div className="flex shrink-0 items-start justify-between gap-2">
                    <div className="min-w-0 space-y-0.5">
                        <p className="text-xs font-medium text-text-secondary">
                            消息内容
                        </p>
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">
                            {isCustom
                                ? '结构未识别，请直接编辑 JSON'
                                : '字段改动会同步到 JSON'}
                        </p>
                    </div>
                    <Badge
                        tone={isCustom ? 'warning' : 'brand'}
                        appearance="soft"
                    >
                        {isCustom ? '自定义' : labelOf(kind)}
                    </Badge>
                </div>

                {isCustom ? (
                    <div className="flex flex-1 flex-col justify-center gap-2 rounded-sm border border-dashed border-border-subtle bg-field/40 px-3 py-6 text-center">
                        <p className="text-[13px] text-text-secondary">
                            当前是自定义 payload
                        </p>
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">
                            可点选服务类型自动套壳，或继续编辑 JSON。
                        </p>
                    </div>
                ) : (
                    <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto overflow-x-hidden p-0.5">
                        <div className="flex flex-wrap items-center gap-1.5">
                            <span className="text-[11px] text-text-tertiary">
                                插入变量
                            </span>
                            {TEMPLATE_VARS.map((v) => (
                                <button
                                    key={v.key}
                                    type="button"
                                    onClick={() => insertVar(`{${v.key}}`)}
                                    className={cn(
                                        'rounded-sm border border-border-subtle bg-field px-1.5 py-0.5',
                                        'text-[11px] text-text-secondary transition-colors',
                                        'hover:border-brand/40 hover:bg-brand/10 hover:text-text',
                                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                    )}
                                    title={`插入 {${v.key}}`}
                                >
                                    {v.label}
                                </button>
                            ))}
                        </div>

                        {meta.showTitle ? (
                            <label className="block min-w-0 space-y-1">
                                <span className="text-xs font-medium text-text-secondary">
                                    {meta.titleLabel}
                                </span>
                                <TextField
                                    name="webhook-msg-title"
                                    autoComplete="off"
                                    value={fields.title}
                                    onValueChange={(v) =>
                                        patchFields({ title: v })
                                    }
                                    onFocus={() => {
                                        activeFieldRef.current = 'title';
                                    }}
                                />
                            </label>
                        ) : null}

                        {meta.showBody ? (
                            <label className="flex min-h-0 min-w-0 flex-1 flex-col gap-1">
                                <span className="text-xs font-medium text-text-secondary">
                                    {meta.bodyLabel}
                                </span>
                                {meta.bodyHint ? (
                                    <span className="text-[11px] text-text-tertiary">
                                        {meta.bodyHint}
                                    </span>
                                ) : null}
                                <textarea
                                    name="webhook-msg-body"
                                    value={fields.body}
                                    rows={6}
                                    spellCheck={false}
                                    onFocus={() => {
                                        activeFieldRef.current = 'body';
                                    }}
                                    onChange={(e) =>
                                        patchFields({ body: e.target.value })
                                    }
                                    className={cn(
                                        'block min-h-[8rem] w-full flex-1 resize-none rounded-sm bg-field px-3 py-2',
                                        'text-sm leading-relaxed text-text',
                                        'border border-border-subtle outline-none transition-colors duration-150',
                                        'placeholder:text-text-tertiary',
                                        'focus-visible:border-brand focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-inset',
                                    )}
                                />
                            </label>
                        ) : null}

                        {meta.showGroup ? (
                            <label className="block min-w-0 space-y-1">
                                <span className="text-xs font-medium text-text-secondary">
                                    分组
                                </span>
                                <TextField
                                    name="webhook-msg-group"
                                    autoComplete="off"
                                    value={fields.group}
                                    onValueChange={(v) =>
                                        patchFields({ group: v })
                                    }
                                    onFocus={() => {
                                        activeFieldRef.current = 'group';
                                    }}
                                />
                            </label>
                        ) : null}
                    </div>
                )}
            </section>

            <section className="flex min-h-0 min-w-0 flex-col gap-2 overflow-hidden rounded-sm border border-border-subtle bg-inset/20 p-3">
                <div className="flex shrink-0 items-start justify-between gap-2">
                    <div className="min-w-0 space-y-0.5">
                        <p className="text-xs font-medium text-text-secondary">
                            原始 JSON
                        </p>
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">
                            可直接编辑；合法时会同步字段
                        </p>
                    </div>
                    <Badge
                        tone={jsonOk ? 'success' : 'danger'}
                        appearance="soft"
                    >
                        {jsonOk ? '合法' : '非法'}
                    </Badge>
                </div>

                <div className="min-h-0 flex-1 p-0.5">
                    <ThemeJsonEditor
                        aria-label="Webhook JSON 模板"
                        value={rawDraft}
                        invalid={!jsonOk}
                        onChange={onRawCommit}
                        className="h-full min-h-[10rem]"
                    />
                </div>
            </section>
        </div>
    );
}

function labelOf(kind: WebhookServiceKind): string {
    switch (kind) {
        case 'serverchan':
            return 'Server酱';
        case 'dingtalk':
            return '钉钉';
        case 'feishu':
            return '飞书';
        case 'discord':
            return 'Discord';
        case 'bark':
            return 'Bark';
        case 'custom':
            return '自定义';
    }
}
