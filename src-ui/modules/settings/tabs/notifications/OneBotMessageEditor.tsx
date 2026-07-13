// OneBot 掉线消息模板编辑 + 预览

import { useRef } from 'react';
import {
    TEMPLATE_VARS,
} from '../../../../core/domain/settings/webhook-message-visual';
import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

function renderTemplatePreview(template: string): string {
    return TEMPLATE_VARS.reduce(
        (text, variable) =>
            text.replaceAll(`{${variable.key}}`, variable.sample),
        template,
    );
}

export function OneBotMessageEditor({
    value,
    onChange,
    onReset,
}: {
    value: string;
    onChange: (value: string) => void;
    onReset: () => void;
}) {
    const editorRef = useRef<HTMLTextAreaElement | null>(null);

    const insertVariable = (token: string) => {
        const editor = editorRef.current;
        const start = editor?.selectionStart ?? value.length;
        const end = editor?.selectionEnd ?? value.length;
        const next = `${value.slice(0, start)}${token}${value.slice(end)}`;
        onChange(next);
        requestAnimationFrame(() => {
            editor?.focus();
            editor?.setSelectionRange(start + token.length, start + token.length);
        });
    };

    const preview = renderTemplatePreview(value);

    return (
        <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-sm border border-border-subtle bg-field">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border-subtle px-3 py-2">
                <p className="text-[13px] font-medium text-text">消息内容</p>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="shrink-0 whitespace-nowrap"
                    onClick={onReset}
                >
                    恢复默认
                </Button>
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-border-subtle bg-inset/30 px-3 py-1.5">
                <span className="mr-0.5 text-[11px] text-text-tertiary">
                    变量
                </span>
                {TEMPLATE_VARS.map((variable) => (
                    <button
                        key={variable.key}
                        type="button"
                        title={`插入 {${variable.key}}`}
                        onClick={() => insertVariable(`{${variable.key}}`)}
                        className={cn(
                            'rounded-sm border border-border-subtle bg-field px-1.5 py-0.5',
                            'text-[11px] text-text-secondary transition-colors',
                            'hover:border-brand/40 hover:bg-brand/10 hover:text-text',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        )}
                    >
                        {variable.label}
                    </button>
                ))}
            </div>

            <textarea
                ref={editorRef}
                aria-label="OneBot 消息模板"
                value={value}
                onChange={(event) => onChange(event.target.value)}
                spellCheck={false}
                autoComplete="off"
                placeholder="输入掉线时要发送的消息…"
                className={cn(
                    'block min-h-[7.5rem] w-full flex-1 resize-none bg-transparent px-3 py-2.5',
                    'font-mono text-[12.5px] leading-relaxed text-text',
                    'outline-none placeholder:text-text-tertiary',
                )}
            />

            <div className="shrink-0 border-t border-border-subtle bg-inset/25 px-3 py-2">
                <div className="mb-1 flex items-center justify-between gap-2">
                    <p className="text-[11px] font-medium text-text-secondary">
                        预览
                    </p>
                    <span className="text-[10.5px] text-text-tertiary">
                        示例数据
                    </span>
                </div>
                <div className="max-h-20 overflow-y-auto whitespace-pre-wrap text-[12px] leading-relaxed text-text-secondary">
                    {preview || (
                        <span className="text-text-tertiary">消息内容为空</span>
                    )}
                </div>
            </div>
        </section>
    );
}
