// 普通文本参数；也给表情 id 这类「数字优先」的字段用（转换规则在 coerceInput 里）。

import { coerceInput } from '../../../../core/domain/debug/schemaForm';
import { cn } from '../../../../shared/utils/cn';
import { FIELD_INPUT_CLASS, fieldBorder } from '../centerParts';
import { valueText } from '../viewHelpers';
import { useTextDraft, type FieldProps } from './fieldKit';

export function TextField({
    field,
    value,
    onChange,
    invalid,
    inputId,
    describedBy,
    disabled,
    placeholder,
    mono,
}: FieldProps & { placeholder?: string; mono?: boolean }) {
    const [text, setText] = useTextDraft(value, valueText, (t) => coerceInput(field, t), onChange);
    return (
        <input
            id={inputId}
            type="text"
            value={text}
            disabled={disabled}
            onChange={(e) => setText(e.target.value)}
            placeholder={
                placeholder ??
                (field.defaultValue !== undefined
                    ? `默认 ${valueText(field.defaultValue)}`
                    : undefined)
            }
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
            spellCheck={false}
            autoComplete="off"
            className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), mono && 'font-mono text-[13px]')}
        />
    );
}
