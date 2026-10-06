// 数字参数。不用浏览器的 number 输入框：它对「1.」「-」这类敲到一半的内容给空值，草稿会丢。
// 敲的是文本，按 coerceInput 转；转不动的原样留字符串，交给校验去标红。

import { coerceInput } from '../../../../core/domain/debug/schemaForm';
import { cn } from '../../../../shared/utils/cn';
import { FIELD_INPUT_CLASS, fieldBorder } from '../centerParts';
import { valueText } from '../viewHelpers';
import { useTextDraft, type FieldProps } from './fieldKit';

function rangeHint(min?: number, max?: number): string | undefined {
    if (min !== undefined && max !== undefined) return `${min} ~ ${max}`;
    if (min !== undefined) return `≥ ${min}`;
    if (max !== undefined) return `≤ ${max}`;
    return undefined;
}

export function NumberField({
    field,
    value,
    onChange,
    invalid,
    inputId,
    describedBy,
    disabled,
}: FieldProps) {
    const [text, setText] = useTextDraft(value, valueText, (t) => coerceInput(field, t), onChange);
    const placeholder =
        field.defaultValue !== undefined
            ? `默认 ${valueText(field.defaultValue)}`
            : (rangeHint(field.minimum, field.maximum) ??
              (field.valueType === 'integer' ? '整数' : '数字'));
    return (
        <input
            id={inputId}
            type="text"
            inputMode={field.valueType === 'integer' ? 'numeric' : 'decimal'}
            value={text}
            disabled={disabled}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
                // 上下键微调：调试时改 duration、times 这类数很常用
                if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
                const n = typeof value === 'number' ? value : Number(text || 0);
                if (!Number.isFinite(n)) return;
                e.preventDefault();
                let next = n + (e.key === 'ArrowUp' ? 1 : -1) * (e.shiftKey ? 10 : 1);
                if (field.minimum !== undefined) next = Math.max(field.minimum, next);
                if (field.maximum !== undefined) next = Math.min(field.maximum, next);
                setText(String(next));
            }}
            placeholder={placeholder}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
            autoComplete="off"
            className={cn(
                FIELD_INPUT_CLASS,
                fieldBorder(invalid),
                'font-mono text-[13px] tabular-nums',
            )}
        />
    );
}
