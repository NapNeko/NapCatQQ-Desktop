// 对象 / 结构未知的参数：内嵌一个小 JSON 编辑器。
// 写到一半不是合法 JSON 时不往参数里写（参数里还是上一份合法值），下面提示一句，不清空用户正在敲的内容。

import { useState } from 'react';
import { JsonCodeEditor } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { prettyJson, sameJson } from '../viewHelpers';
import type { FieldProps } from './fieldKit';

const toText = (v: unknown) => (v === undefined ? '' : prettyJson(v));

export function JsonField({
    value,
    onChange,
    invalid,
    inputId,
    onSubmit,
    heightClass = 'h-24',
    ariaLabel,
}: FieldProps & { heightClass?: string; ariaLabel?: string }) {
    const [state, setState] = useState(() => ({ text: toText(value), value, broken: false }));
    let current = state;
    if (!sameJson(state.value, value)) {
        current = { text: toText(value), value, broken: false };
        setState(current);
    }

    const onText = (text: string) => {
        if (text.trim() === '') {
            setState({ text, value: undefined, broken: false });
            onChange(undefined);
            return;
        }
        try {
            const parsed: unknown = JSON.parse(text);
            setState({ text, value: parsed, broken: false });
            onChange(parsed);
        } catch {
            setState((s) => ({ ...s, text, broken: true }));
        }
    };

    return (
        <div id={inputId} className="flex flex-col gap-1">
            <JsonCodeEditor
                value={current.text}
                onChange={onText}
                onSubmit={onSubmit}
                ariaLabel={ariaLabel}
                className={cn(heightClass, 'flex-none', (invalid || current.broken) && 'border-danger')}
            />
            {current.broken && <p className="text-2xs text-warning">这里的 JSON 还没写完整，参数里仍是上一次的合法值</p>}
        </div>
    );
}
