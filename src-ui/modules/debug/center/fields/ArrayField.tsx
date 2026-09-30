// 标量数组参数（一串 QQ 号、一串消息 id……）：每项一个输入框，能增删。
// 「添加」先给一个空行，敲下第一个字才真的写进参数，空行留着不会往 JSON 里塞一个 ""。
// 往空行里粘贴「1,2,3」或多行文本时按分隔拆成多项。

import { useState, type ClipboardEvent, type KeyboardEvent } from 'react';
import { Plus, X } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';
import { FIELD_INPUT_CLASS, IconTip, fieldBorder } from '../centerParts';
import { valueText } from '../viewHelpers';
import { useTextDraft, type FieldProps } from './fieldKit';

const NUMBER_TEXT = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i;

function coerceItem(kind: 'text' | 'number' | undefined, raw: string): unknown {
    if (kind !== 'number') return raw;
    const t = raw.trim();
    if (!NUMBER_TEXT.test(t)) return raw;
    const n = Number(t);
    // 长 id 超出安全整数范围时留字符串，别悄悄丢精度
    return Number.isFinite(n) && (Number.isSafeInteger(n) || !Number.isInteger(n)) ? n : raw;
}

function ItemInput({
    id,
    value,
    kind,
    invalid,
    disabled,
    autoFocus,
    placeholder,
    onCommit,
    onEnter,
    onBlurEmpty,
    onPasteMany,
    describedBy,
}: {
    id?: string;
    value: unknown;
    kind: 'text' | 'number' | undefined;
    invalid: boolean;
    disabled?: boolean;
    autoFocus?: boolean;
    placeholder?: string;
    onCommit: (v: unknown) => void;
    onEnter: () => void;
    onBlurEmpty?: () => void;
    onPasteMany?: (parts: string[]) => void;
    describedBy?: string;
}) {
    const [text, setText] = useTextDraft(value, valueText, (t) => coerceItem(kind, t), onCommit);
    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            onEnter();
        }
    };
    const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
        if (!onPasteMany) return;
        const pasted = e.clipboardData.getData('text');
        const parts = pasted.split(/[\n,，]/).map((s) => s.trim()).filter(Boolean);
        if (parts.length < 2) return;
        e.preventDefault();
        onPasteMany(parts);
    };
    return (
        <input
            id={id}
            type="text"
            value={text}
            disabled={disabled}
            autoFocus={autoFocus}
            placeholder={placeholder}
            inputMode={kind === 'number' ? 'numeric' : undefined}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
            onPaste={onPaste}
            onBlur={() => {
                if (text === '') onBlurEmpty?.();
            }}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
            spellCheck={false}
            autoComplete="off"
            className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), 'h-8 font-mono text-[13px]')}
        />
    );
}

export function ArrayField({ field, value, onChange, invalid, inputId, describedBy, disabled }: FieldProps) {
    const [pending, setPending] = useState(false);
    const kind = field.itemKind;

    if (value !== undefined && value !== null && !Array.isArray(value)) {
        return (
            <div className="flex min-h-[38px] flex-wrap items-center gap-2 text-xs text-text-secondary">
                <span>现在的值不是数组（{valueText(value).slice(0, 40)}）。</span>
                <button
                    type="button"
                    onClick={() => onChange([])}
                    className="text-brand hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                >
                    换成空列表
                </button>
            </div>
        );
    }

    const items: unknown[] = Array.isArray(value) ? value : [];
    const setAt = (i: number, v: unknown) => {
        const next = items.slice();
        next[i] = v;
        onChange(next);
    };
    const removeAt = (i: number) => {
        const next = items.filter((_, j) => j !== i);
        onChange(next.length === 0 && !field.required ? undefined : next);
    };
    const appendMany = (parts: string[]) => {
        onChange([...items, ...parts.map((p) => coerceItem(kind, p))]);
        setPending(false);
    };

    // 已有的项和「正在敲的新项」放在同一个数组里：新项写进参数后，它和原来那一行的键相同，
    // React 复用同一个输入框，焦点不丢
    const rows = items.map((item, i) => (
        <div key={i} className="flex items-center gap-1">
            <span className="w-5 shrink-0 text-right font-mono text-[10.5px] text-text-tertiary">{i}</span>
            <ItemInput
                id={i === 0 ? inputId : undefined}
                value={item}
                kind={kind}
                invalid={invalid}
                disabled={disabled}
                describedBy={describedBy}
                onCommit={(v) => setAt(i, v)}
                onEnter={() => setPending(true)}
            />
            <IconTip icon={X} label={`删掉第 ${i} 项`} size="sm" tone="danger" disabled={disabled} onClick={() => removeAt(i)} />
        </div>
    ));
    if (pending) {
        rows.push(
            <div key={items.length} className="flex items-center gap-1">
                <span className="w-5 shrink-0 text-right font-mono text-[10.5px] text-text-tertiary">{items.length}</span>
                <ItemInput
                    id={items.length === 0 ? inputId : undefined}
                    value={undefined}
                    kind={kind}
                    invalid={false}
                    disabled={disabled}
                    autoFocus
                    placeholder={kind === 'number' ? '数字，可粘贴一串用逗号隔开' : '可粘贴多行一次加好几项'}
                    onCommit={(v) => {
                        if (v === '' || v === undefined) return;
                        onChange([...items, v]);
                        setPending(false);
                    }}
                    onEnter={() => undefined}
                    onBlurEmpty={() => setPending(false)}
                    onPasteMany={appendMany}
                />
                <span className="w-6 shrink-0" />
            </div>,
        );
    }

    return (
        <div className="flex flex-col gap-1.5">
            {rows}
            <div className="flex items-center gap-2">
                <button
                    type="button"
                    id={items.length === 0 && !pending ? inputId : undefined}
                    disabled={disabled}
                    onClick={() => setPending(true)}
                    className={cn(
                        'inline-flex h-7 items-center gap-1 rounded-sm border border-dashed border-border px-2 text-xs text-text-secondary transition-colors',
                        'hover:border-brand/60 hover:text-brand focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                    )}
                >
                    <Plus size={12} aria-hidden />
                    添加一项
                </button>
                {items.length > 0 && <span className="text-2xs text-text-tertiary">{items.length} 项</span>}
            </div>
        </div>
    );
}
