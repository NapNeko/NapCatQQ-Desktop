// 字典字段：键值对的增删改；键名失焦才提交（DictKeyInput）。

import { useState } from 'react';
import { Plus } from 'lucide-react';
import { Button, TextField } from '../../../../../shared/ui';
import { cn } from '../../../../../shared/utils/cn';
import { blankOf, isObjectLike, type SNode } from '../../../../../core/domain/apps/koishiSchema';
import { asObj, withKey, type Obj } from '../schemasteryModel';
import { SchemaField } from './SchemaField';
import { SchemaObject } from './SchemaObject';
import { NestedCard } from './NestedCard';
import { ItemRemove } from './ItemRemove';

/** 字典的键：边打边改会让整行换 key 重挂、丢焦点，所以失焦再改名 */
function DictKeyInput({
    value,
    onCommit,
    disabled,
}: {
    value: string;
    onCommit: (next: string) => void;
    disabled?: boolean;
}) {
    const [draft, setDraft] = useState(value);
    return (
        <TextField
            label="键"
            value={draft}
            disabled={disabled}
            onValueChange={setDraft}
            onBlur={() => {
                const next = draft.trim();
                if (next && next !== value) onCommit(next);
                else setDraft(value);
            }}
        />
    );
}

export function DictField({
    name,
    node,
    value,
    onChange,
    disabled,
    canReset,
}: {
    name: string;
    node: SNode;
    value: unknown;
    onChange: (next: unknown) => void;
    disabled?: boolean;
    canReset: boolean;
}) {
    const obj = asObj(value);
    const entries = Object.entries(obj);
    const inner = node.inner;
    const [draftKey, setDraftKey] = useState('');
    if (!inner) return null;
    const rename = (from: string, to: string) => {
        if (!to || to === from || to in obj) return;
        const next: Obj = {};
        for (const [k, v] of entries) next[k === from ? to : k] = v;
        onChange(next);
    };
    const key = draftKey.trim();
    return (
        <NestedCard
            name={name}
            node={node}
            onReset={canReset ? () => onChange(undefined) : undefined}
            disabled={disabled}
            count={entries.length}
        >
            <ul className="flex flex-col divide-y divide-border-subtle/70">
                {entries.map(([k, v]) => (
                    <li key={k} className="flex items-start gap-3 py-3 first:pt-0">
                        <div className="w-44 shrink-0">
                            <DictKeyInput
                                value={k}
                                disabled={disabled}
                                onCommit={(to) => rename(k, to)}
                            />
                        </div>
                        <div className="grid min-w-0 flex-1 gap-3">
                            {isObjectLike(inner) ? (
                                <SchemaObject
                                    node={inner}
                                    value={asObj(v)}
                                    disabled={disabled}
                                    onChange={(nv) => onChange({ ...obj, [k]: nv })}
                                />
                            ) : (
                                <SchemaField
                                    name="值"
                                    node={inner}
                                    value={v}
                                    resettable={false}
                                    disabled={disabled}
                                    onChange={(nv) => onChange({ ...obj, [k]: nv })}
                                />
                            )}
                        </div>
                        <ItemRemove
                            disabled={disabled}
                            onClick={() => onChange(withKey(obj, k, undefined))}
                        />
                    </li>
                ))}
            </ul>
            <div
                className={cn(
                    'flex items-center gap-2',
                    entries.length > 0 && 'mt-3 border-t border-border-subtle/70 pt-3',
                )}
            >
                <div className="w-44">
                    <TextField
                        value={draftKey}
                        placeholder="新的键"
                        disabled={disabled}
                        onValueChange={setDraftKey}
                    />
                </div>
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={disabled || !key || key in obj}
                    onClick={() => {
                        onChange({ ...obj, [key]: blankOf(inner) });
                        setDraftKey('');
                    }}
                >
                    <Plus size={13} />
                    加一项
                </Button>
            </div>
        </NestedCard>
    );
}
