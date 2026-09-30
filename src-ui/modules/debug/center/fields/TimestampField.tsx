// 时间戳参数：左边挑日期时间，右边是 Unix 秒，两边同步。上游要的是秒。

import { coerceInput } from '../../../../core/domain/debug/schemaForm';
import { cn } from '../../../../shared/utils/cn';
import { FIELD_INPUT_CLASS, fieldBorder } from '../centerParts';
import { localInputToSeconds, secondsToLocalInput, valueText } from '../viewHelpers';
import { useTextDraft, type FieldProps } from './fieldKit';

export function TimestampField({ field, value, onChange, invalid, inputId, describedBy, disabled }: FieldProps) {
    const [text, setText] = useTextDraft(value, valueText, (t) => coerceInput(field, t), onChange);
    const local = secondsToLocalInput(value);
    return (
        <div className="grid grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)] gap-1.5">
            <input
                type="datetime-local"
                step={1}
                value={local}
                disabled={disabled}
                aria-label={`${field.name} 的日期时间`}
                onChange={(e) => {
                    const s = localInputToSeconds(e.target.value);
                    setText(s === null ? '' : String(s));
                }}
                className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), 'px-2 text-[13px] tabular-nums')}
            />
            <input
                id={inputId}
                type="text"
                inputMode="numeric"
                value={text}
                disabled={disabled}
                placeholder="Unix 秒"
                onChange={(e) => setText(e.target.value)}
                aria-invalid={invalid || undefined}
                aria-describedby={describedBy}
                autoComplete="off"
                className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), 'font-mono text-[13px] tabular-nums')}
            />
        </div>
    );
}
