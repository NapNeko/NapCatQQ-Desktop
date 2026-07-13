// 目标 ID 多值输入：chip + 逗号/回车确认

import { useRef, useState } from 'react';
import { X } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';

function parseTargetTokens(raw: string): number[] {
    return raw
        .split(/[,，;；\s]+/)
        .map((part) => Number(part.trim()))
        .filter((n) => Number.isFinite(n) && n > 0)
        .map((n) => Math.round(n));
}

function Chip({
    label,
    onRemove,
    tone = 'neutral',
}: {
    label: string;
    onRemove: () => void;
    tone?: 'neutral' | 'success' | 'warning';
}) {
    return (
        <span
            className={cn(
                'inline-flex max-w-full items-center gap-1 rounded-sm border px-2 py-1 text-[12px]',
                tone === 'success' &&
                'border-success/30 bg-success-soft text-text',
                tone === 'warning' &&
                'border-warning/30 bg-warning-soft text-text',
                tone === 'neutral' &&
                'border-border-subtle bg-inset text-text-secondary',
            )}
        >
            <span className="truncate">{label}</span>
            <button
                type="button"
                aria-label={`移除 ${label}`}
                onClick={onRemove}
                className="rounded-xs p-0.5 text-text-tertiary transition-colors hover:bg-field hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                <X size={12} />
            </button>
        </span>
    );
}

export function TargetIdChipsInput({
    values,
    onChange,
    placeholder,
    className,
}: {
    values: number[];
    onChange: (next: number[]) => void;
    placeholder?: string;
    className?: string;
}) {
    const [draft, setDraft] = useState('');
    const inputRef = useRef<HTMLInputElement | null>(null);

    const commitDraft = (raw: string) => {
        const tokens = parseTargetTokens(raw);
        if (tokens.length === 0) {
            setDraft('');
            return;
        }
        const next = [...values];
        for (const token of tokens) {
            if (!next.includes(token)) next.push(token);
        }
        onChange(next);
        setDraft('');
    };

    return (
        <div
            className={cn(
                'flex min-h-10 w-full content-start items-start gap-1.5 overflow-y-auto rounded-sm border border-border-subtle bg-field px-2 py-1.5',
                'flex-wrap focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                className,
            )}
            onClick={() => inputRef.current?.focus()}
        >
            {values.map((id) => (
                <Chip
                    key={id}
                    label={String(id)}
                    onRemove={() => onChange(values.filter((item) => item !== id))}
                />
            ))}
            <input
                ref={inputRef}
                value={draft}
                placeholder={values.length === 0 ? placeholder : '继续输入，逗号确认'}
                onChange={(event) => {
                    const next = event.target.value;
                    if (/[,，;；\n]/.test(next)) {
                        commitDraft(next);
                        return;
                    }
                    setDraft(next);
                }}
                onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ',' || event.key === '，') {
                        event.preventDefault();
                        commitDraft(draft);
                        return;
                    }
                    if (event.key === 'Backspace' && !draft && values.length > 0) {
                        onChange(values.slice(0, -1));
                    }
                }}
                onBlur={() => {
                    if (draft.trim()) commitDraft(draft);
                }}
                className="min-w-[9rem] flex-1 bg-transparent py-0.5 text-sm text-text outline-none placeholder:text-text-tertiary"
            />
        </div>
    );
}
