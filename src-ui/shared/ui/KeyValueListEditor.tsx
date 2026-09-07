// 自定义键：主行 key / value / 删除，注释单独一行，避免四列挤爆。

import { Plus, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { cn } from '../utils/cn';
import { Button } from './Button';
import { TextField } from './TextField';

export interface KeyValueEntry {
    key: string;
    value: string;
    comment: string;
}

export interface KeyValueListEditorProps {
    value: KeyValueEntry[];
    onChange: (next: KeyValueEntry[]) => void;
    keyErrors?: Record<number, string | undefined>;
    disabled?: boolean;
    emptyHint?: ReactNode;
    addLabel?: string;
    className?: string;
}

export const KeyValueListEditor: React.FC<KeyValueListEditorProps> = ({
    value,
    onChange,
    keyErrors,
    disabled,
    emptyHint,
    addLabel = '添加',
    className,
}) => {
    const update = (idx: number, patch: Partial<KeyValueEntry>) =>
        onChange(value.map((e, i) => (i === idx ? { ...e, ...patch } : e)));

    return (
        <div className={cn('flex flex-col', className)}>
            {value.length === 0 && emptyHint && (
                <p className="py-2 text-xs text-text-tertiary">{emptyHint}</p>
            )}
            <div className="flex flex-col divide-y divide-border-subtle/70">
                {value.map((entry, idx) => (
                    <div key={idx} className="flex flex-col gap-3 py-4 first:pt-1 last:pb-1">
                        <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_auto] items-start gap-3">
                            <TextField
                                aria-label="键"
                                placeholder="KEY"
                                value={entry.key}
                                disabled={disabled}
                                error={keyErrors?.[idx]}
                                className="font-mono"
                                onValueChange={(v) => update(idx, { key: v.toUpperCase() })}
                            />
                            <TextField
                                aria-label="值"
                                placeholder="value"
                                value={entry.value}
                                disabled={disabled}
                                className="font-mono"
                                onValueChange={(v) => update(idx, { value: v })}
                            />
                            <Button
                                variant="ghost"
                                size="icon"
                                aria-label="删除"
                                disabled={disabled}
                                onClick={() => onChange(value.filter((_, i) => i !== idx))}
                            >
                                <Trash2 size={15} />
                            </Button>
                        </div>
                        <TextField
                            aria-label="注释"
                            placeholder="# 注释（可选）"
                            value={entry.comment}
                            disabled={disabled}
                            onValueChange={(v) => update(idx, { comment: v })}
                        />
                    </div>
                ))}
            </div>
            <div className="pt-4">
                <Button
                    variant="secondary"
                    size="sm"
                    disabled={disabled}
                    onClick={() => onChange([...value, { key: '', value: '', comment: '' }])}
                >
                    <Plus size={13} /> {addLabel}
                </Button>
            </div>
        </div>
    );
};
