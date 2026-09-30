// 消息参数：「文本」直接写一段字（上游按 CQ 码解析），「消息段 JSON」写数组。
// 文本换成 JSON 时包成一个 text 段；JSON 换回文本只在全是 text 段时可以，不然会丢掉图片、@ 这些段。

import { useState } from 'react';
import { Segmented } from '../centerParts';
import { cn } from '../../../../shared/utils/cn';
import { fieldBorder } from '../centerParts';
import { segmentsToText, textToSegments, valueText } from '../viewHelpers';
import { JsonField } from './JsonField';
import { useTextDraft, type FieldProps } from './fieldKit';

type Mode = 'text' | 'json';

export function MessageField(props: FieldProps) {
    const { value, onChange, invalid, inputId, describedBy, disabled } = props;
    // 值本身的形状说了算（JSON 视图里改成数组，这边就跟着切）；没填时用上次选的
    const [preferred, setPreferred] = useState<Mode>(Array.isArray(value) ? 'json' : 'text');
    const mode: Mode = Array.isArray(value) ? 'json' : typeof value === 'string' ? 'text' : preferred;
    const backToText = mode === 'json' ? segmentsToText(value ?? []) : '';
    const canText = mode === 'text' || backToText !== null;

    const switchTo = (next: Mode) => {
        if (next === mode) return;
        setPreferred(next);
        if (next === 'json') {
            if (typeof value === 'string') onChange(textToSegments(value));
        } else if (backToText !== null && value !== undefined) {
            onChange(backToText === '' ? undefined : backToText);
        }
    };

    return (
        <div className="flex flex-col gap-1.5">
            <div className="flex items-center gap-2">
                <Segmented<Mode>
                    label="消息的写法"
                    size="xs"
                    value={mode}
                    onChange={switchTo}
                    options={[
                        {
                            value: 'text',
                            label: '文本',
                            disabled: !canText || disabled,
                            title: canText ? undefined : '消息里有图片、@ 之类的非文本段，换成文本会丢内容',
                        },
                        { value: 'json', label: '消息段 JSON', disabled },
                    ]}
                />
                <span className="truncate text-2xs text-text-tertiary">
                    {mode === 'text' ? '按 CQ 码解析，比如 [CQ:face,id=14]' : '数组，每项形如 {"type":"text","data":{"text":"…"}}'}
                </span>
            </div>
            {mode === 'text' ? (
                <MessageText
                    value={value}
                    onChange={onChange}
                    invalid={invalid}
                    inputId={inputId}
                    describedBy={describedBy}
                    disabled={disabled}
                />
            ) : (
                <JsonField {...props} heightClass="h-[120px]" ariaLabel="消息段 JSON" />
            )}
        </div>
    );
}

function MessageText({
    value,
    onChange,
    invalid,
    inputId,
    describedBy,
    disabled,
}: Pick<FieldProps, 'value' | 'onChange' | 'invalid' | 'inputId' | 'describedBy' | 'disabled'>) {
    const [text, setText] = useTextDraft(value, valueText, (t) => (t === '' ? undefined : t), onChange);
    return (
        <textarea
            id={inputId}
            value={text}
            rows={3}
            disabled={disabled}
            onChange={(e) => setText(e.target.value)}
            placeholder="要发的文字"
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
            className={cn(
                'block min-h-[72px] w-full resize-y rounded-sm border bg-field px-3 py-2 text-sm text-text outline-none transition-colors',
                'placeholder:text-text-tertiary focus:ring-2 focus:ring-inset disabled:cursor-not-allowed disabled:bg-inset',
                fieldBorder(invalid),
            )}
        />
    );
}
