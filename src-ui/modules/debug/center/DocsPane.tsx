// 「文档」子页：说明、参数表、返回结构、示例（可一键填进参数）、常见错误、SnowLuma 的约束、NC ↔ SL 差异、目录来源。

import { memo, useMemo, useState, type ReactNode } from 'react';
import { Check, CornerDownLeft } from 'lucide-react';
import { JsonTree, SimpleMarkdown } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { buildFormModel } from '../../../core/domain/debug/schemaForm';
import { formatParams } from '../../../core/domain/debug/paramsText';
import { retcodeHint } from '../../../core/domain/debug/errorCopy';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugParamDiffKind } from '../../../core/ipc/generated/debug/DebugParamDiffKind';
import type { BackendType } from '../../../core/ipc/generated/domain/BackendType';
import {
    ROLE_LABEL,
    countTreeRows,
    prettyJson,
    schemaRole,
    schemaTypeText,
    simplifySchema,
    valueText,
} from './viewHelpers';

const BACKEND_NAME: Record<BackendType, string> = { napcat: 'NapCat', snowluma: 'SnowLuma' };

function diffText(d: DebugParamDiffKind, here: string, other: string): string {
    switch (d.kind) {
        case 'only_here':
            return `只有 ${here} 有`;
        case 'only_other':
            return `只有 ${other} 有`;
        case 'type_differs':
            return `类型不同：${here} 是 ${d.here}，${other} 是 ${d.other}`;
        case 'required_differs':
            return `${here} ${d.here ? '必填' : '选填'}，${other} ${d.other ? '必填' : '选填'}`;
    }
}

function Section({
    title,
    children,
    aside,
}: {
    title: string;
    children: ReactNode;
    aside?: ReactNode;
}) {
    return (
        <section className="space-y-1.5">
            <div className="flex items-baseline gap-2">
                <h3 className="font-display text-[12.5px] font-semibold text-text">{title}</h3>
                {aside}
            </div>
            {children}
        </section>
    );
}

/** JSON 树要一个确定的高度：按行数给，最多 320px，内容多了在里面滚 */
function SizedTree({ value, depth = 3 }: { value: unknown; depth?: number }) {
    const rows = countTreeRows(value);
    return (
        <div style={{ height: Math.min(320, rows * 22 + 10) }} className="flex">
            <JsonTree value={value} defaultExpandDepth={depth} />
        </div>
    );
}

export interface DocsPaneProps {
    spec: DebugActionSpec;
    backend: BackendType | null;
    /** 当前参数被用户改过：套用示例前要先问一声 */
    dirty: boolean;
    onUseExample: (text: string) => void;
}

export const DocsPane = memo(function DocsPane({
    spec,
    backend,
    dirty,
    onUseExample,
}: DocsPaneProps) {
    const model = useMemo(() => buildFormModel(spec.params_schema), [spec.params_schema]);
    const properties = (spec.params_schema.properties ?? {}) as Record<string, unknown>;
    const returns = useMemo(
        () => (spec.returns_schema ? simplifySchema(spec.returns_schema) : null),
        [spec.returns_schema],
    );
    const examples = spec.examples.filter(
        (e): e is Record<string, unknown> =>
            typeof e === 'object' && e !== null && !Array.isArray(e),
    );
    const here = backend ? BACKEND_NAME[backend] : '这边';
    const other = spec.other_backend ? BACKEND_NAME[spec.other_backend.backend] : '另一边';

    return (
        <div className="space-y-5 px-3 py-3 text-xs">
            <section className="space-y-1.5">
                <p className="text-[13px] leading-relaxed text-text">
                    {spec.summary || '（上游没有写简介）'}
                </p>
                {spec.description && spec.description.trim() !== spec.summary.trim() && (
                    <SimpleMarkdown text={spec.description} className="text-xs leading-relaxed" />
                )}
                {spec.aliases.length > 0 && (
                    <p className="text-2xs text-text-tertiary">
                        别名：<span className="font-mono">{spec.aliases.join('、')}</span>
                    </p>
                )}
            </section>

            <Section
                title="参数"
                aside={
                    <span className="text-2xs text-text-tertiary">{model.fields.length} 个</span>
                }
            >
                {model.fields.length === 0 ? (
                    <p className="text-text-tertiary">不需要参数。</p>
                ) : (
                    <div
                        role="table"
                        aria-label="参数表"
                        className="overflow-hidden rounded-sm border border-border-subtle"
                    >
                        <div
                            role="row"
                            className="grid grid-cols-[minmax(88px,30%)_minmax(64px,20%)_1fr] bg-inset text-2xs font-medium text-text-secondary"
                        >
                            <span role="columnheader" className="px-2 py-1.5">
                                参数
                            </span>
                            <span role="columnheader" className="px-2 py-1.5">
                                类型
                            </span>
                            <span role="columnheader" className="px-2 py-1.5">
                                说明
                            </span>
                        </div>
                        {model.fields.map((f) => {
                            const raw = properties[f.name];
                            const role = schemaRole(raw);
                            return (
                                <div
                                    key={f.name}
                                    role="row"
                                    className="grid grid-cols-[minmax(88px,30%)_minmax(64px,20%)_1fr] border-t border-border-subtle/70"
                                >
                                    <span role="cell" className="min-w-0 space-y-0.5 px-2 py-1.5">
                                        <span className="block break-all font-mono text-[12px] text-text">
                                            {f.name}
                                        </span>
                                        <span className="flex flex-wrap gap-1">
                                            <span
                                                className={cn(
                                                    'text-[10px]',
                                                    f.required
                                                        ? 'text-danger'
                                                        : 'text-text-tertiary',
                                                )}
                                            >
                                                {f.required ? '必填' : '选填'}
                                            </span>
                                            {role && ROLE_LABEL[role] && (
                                                <span className="rounded-xs bg-brand-soft px-1 text-[10px] text-brand">
                                                    {ROLE_LABEL[role]}
                                                </span>
                                            )}
                                        </span>
                                    </span>
                                    <span
                                        role="cell"
                                        className="min-w-0 break-all px-2 py-1.5 font-mono text-[11px] text-text-secondary"
                                    >
                                        {schemaTypeText(raw)}
                                    </span>
                                    <span
                                        role="cell"
                                        className="min-w-0 space-y-0.5 px-2 py-1.5 text-text-secondary"
                                    >
                                        {f.description && (
                                            <span className="block leading-relaxed">
                                                {f.description}
                                            </span>
                                        )}
                                        {f.defaultValue !== undefined && (
                                            <span className="block text-2xs text-text-tertiary">
                                                默认{' '}
                                                <code className="font-mono text-text-secondary">
                                                    {valueText(f.defaultValue) || '""'}
                                                </code>
                                            </span>
                                        )}
                                        {f.enumValues && f.enumValues.length > 0 && (
                                            <span className="block text-2xs text-text-tertiary">
                                                可选{' '}
                                                {f.enumValues.map((e, i) => (
                                                    <code
                                                        key={i}
                                                        className="mr-1 font-mono text-text-secondary"
                                                    >
                                                        {JSON.stringify(e.value)}
                                                    </code>
                                                ))}
                                            </span>
                                        )}
                                        {!f.description &&
                                            f.defaultValue === undefined &&
                                            !f.enumValues && (
                                                <span className="text-text-disabled">—</span>
                                            )}
                                    </span>
                                </div>
                            );
                        })}
                    </div>
                )}
            </Section>

            <Section title="返回">
                {typeof returns === 'string' ? (
                    // 返回值本身是个标量（null、boolean……）：一行字比一棵只有根的树好读
                    <code className="block rounded-sm bg-inset px-2.5 py-1.5 font-mono text-[12px] text-text-secondary">
                        {returns}
                    </code>
                ) : returns !== null ? (
                    <SizedTree value={returns} />
                ) : spec.returns_text ? (
                    <p className="leading-relaxed text-text-secondary">{spec.returns_text}</p>
                ) : (
                    <p className="text-text-tertiary">上游没有写返回结构。</p>
                )}
                {spec.return_example !== null && spec.return_example !== undefined && (
                    <details className="group">
                        <summary className="cursor-pointer select-none text-2xs text-text-secondary hover:text-text">
                            返回示例
                        </summary>
                        <div className="mt-1.5">
                            <SizedTree value={spec.return_example} depth={2} />
                        </div>
                    </details>
                )}
            </Section>

            {examples.length > 0 && (
                <Section title="请求示例">
                    <div className="space-y-2">
                        {examples.map((ex, i) => (
                            <ExampleCard key={i} example={ex} dirty={dirty} onUse={onUseExample} />
                        ))}
                    </div>
                </Section>
            )}

            {spec.error_examples.length > 0 && (
                <Section title="常见错误">
                    <ul className="space-y-1">
                        {spec.error_examples.map((e, i) => (
                            <li key={i} className="flex gap-2">
                                <code className="shrink-0 rounded-xs bg-danger-soft px-1 font-mono text-[11px] text-danger">
                                    {e.retcode}
                                </code>
                                <span className="min-w-0 text-text-secondary">
                                    {e.message}
                                    {retcodeHint(e.retcode) && (
                                        <span className="block text-2xs text-text-tertiary">
                                            {retcodeHint(e.retcode)}
                                        </span>
                                    )}
                                </span>
                            </li>
                        ))}
                    </ul>
                </Section>
            )}

            {spec.invariants.length > 0 && (
                <Section title="约束">
                    <ul className="list-disc space-y-0.5 pl-4 text-text-secondary">
                        {spec.invariants.map((line, i) => (
                            <li key={i}>{line}</li>
                        ))}
                    </ul>
                </Section>
            )}

            {spec.other_backend && (
                <Section title={`和 ${other} 对照`}>
                    {!spec.other_backend.present ? (
                        <p className="text-text-secondary">{other} 没有这个接口。</p>
                    ) : spec.other_backend.diffs.length === 0 ? (
                        <p className="text-text-secondary">{other} 上也有，参数一致。</p>
                    ) : (
                        <div className="overflow-hidden rounded-sm border border-border-subtle">
                            {spec.other_backend.diffs.map((d, i) => (
                                <div
                                    key={i}
                                    className={cn(
                                        'flex gap-3 px-2 py-1.5',
                                        i > 0 && 'border-t border-border-subtle/70',
                                    )}
                                >
                                    <span className="w-[30%] shrink-0 break-all font-mono text-[12px] text-text">
                                        {d.name}
                                    </span>
                                    <span className="min-w-0 text-text-secondary">
                                        {diffText(d.diff, here, other)}
                                    </span>
                                </div>
                            ))}
                        </div>
                    )}
                </Section>
            )}

            <p className="border-t border-border-subtle/70 pt-2 text-2xs text-text-tertiary">
                {spec.source === 'live'
                    ? '说明来自这个 Bot 在线提供的目录。'
                    : '说明来自桌面端内置的目录，可能和你的 Bot 版本有出入。'}
                {!spec.supported && ' 当前 Bot 不支持这个接口，调用多半会返回「不支持的 API」。'}
            </p>
        </div>
    );
});

function ExampleCard({
    example,
    dirty,
    onUse,
}: {
    example: Record<string, unknown>;
    dirty: boolean;
    onUse: (text: string) => void;
}) {
    const [confirming, setConfirming] = useState(false);
    const text = formatParams(example);
    return (
        <div className="overflow-hidden rounded-sm border border-border-subtle">
            <pre className="max-h-48 overflow-auto bg-inset px-2.5 py-2 font-mono text-[11.5px] leading-relaxed text-text">
                {prettyJson(example)}
            </pre>
            <div className="flex items-center justify-end gap-2 border-t border-border-subtle/70 px-2 py-1.5">
                {confirming ? (
                    <>
                        <span className="mr-auto text-2xs text-warning">
                            会覆盖你改过的参数，确定？
                        </span>
                        <button
                            type="button"
                            onClick={() => setConfirming(false)}
                            className="rounded-xs px-2 py-1 text-2xs text-text-secondary hover:bg-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                        >
                            算了
                        </button>
                        <button
                            type="button"
                            autoFocus
                            onClick={() => {
                                setConfirming(false);
                                onUse(text);
                            }}
                            className="inline-flex items-center gap-1 rounded-xs bg-warning-soft px-2 py-1 text-2xs font-medium text-warning hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                        >
                            <Check size={11} aria-hidden />
                            覆盖
                        </button>
                    </>
                ) : (
                    <button
                        type="button"
                        onClick={() => (dirty ? setConfirming(true) : onUse(text))}
                        className="inline-flex items-center gap-1 rounded-xs px-2 py-1 text-2xs font-medium text-brand hover:bg-brand-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                    >
                        <CornerDownLeft size={11} aria-hidden />
                        用这个示例
                    </button>
                )}
            </div>
        </div>
    );
}
