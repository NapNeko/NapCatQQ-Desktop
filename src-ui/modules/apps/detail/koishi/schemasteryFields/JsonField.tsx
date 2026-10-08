// 画不了的节点：JSON 原文编辑框。

import { useState } from 'react';
import { TextAreaField } from '../../../../../shared/ui';
import type { SNode } from '../../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText } from '../schemaLabel';
import { WIDE } from '../schemasteryModel';

/** 画不了的节点：JSON 原文，失焦时解析，解析不了就不写回 */
export function JsonField({
    name,
    node,
    value,
    onChange,
    disabled,
}: {
    name: string;
    node: SNode;
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
}) {
    const [text, setText] = useState(() =>
        value === undefined ? '' : JSON.stringify(value, null, 2),
    );
    const [error, setError] = useState<string | null>(null);
    const { hint } = fieldText(name, node);
    return (
        <TextAreaField
            className={WIDE}
            label={<FieldLabel name={name} node={node} />}
            hint={error ?? `${hint ? `${hint} ` : ''}这一项表单画不了，按 JSON 填；留空用默认值`}
            error={error ?? undefined}
            value={text}
            mono
            minRows={3}
            disabled={disabled}
            onValueChange={setText}
            onBlur={() => {
                if (!text.trim()) {
                    setError(null);
                    onChange(undefined);
                    return;
                }
                try {
                    onChange(JSON.parse(text));
                    setError(null);
                } catch (e) {
                    setError(`不是合法的 JSON：${(e as Error).message}`);
                }
            }}
        />
    );
}
