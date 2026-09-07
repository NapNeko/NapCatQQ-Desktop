// 字符串列表输入：chips + 回车 / 逗号添加 + 删除 + 粘贴按逗号、空格、换行拆分。
// 供 master / admin / 黑白名单 / 别名 / 插件启停名单这类「一串 id」字段用。
// 受控：value 是 string[]，去重与 trim 由本组件负责。

import { useId, useRef, useState, type KeyboardEvent, type ClipboardEvent, type ReactNode } from 'react';
import { X } from 'lucide-react';
import { cn } from '../utils/cn';

export interface StringListFieldProps {
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    value: string[];
    onChange: (next: string[]) => void;
    placeholder?: string;
    disabled?: boolean;
    className?: string;
    /** 等宽显示（id / 插件名） */
    mono?: boolean;
}

const SPLIT_RE = /[\s,，;；]+/;

function normalize(items: string[], existing: string[]): string[] {
    const seen = new Set(existing);
    const out: string[] = [];
    for (const raw of items) {
        const v = raw.trim();
        if (!v || seen.has(v)) continue;
        seen.add(v);
        out.push(v);
    }
    return out;
}

export const StringListField: React.FC<StringListFieldProps> = ({
    label,
    hint,
    error,
    value,
    onChange,
    placeholder = '输入后回车添加',
    disabled,
    className,
    mono = true,
}) => {
    const [draft, setDraft] = useState('');
    const inputRef = useRef<HTMLInputElement | null>(null);
    const id = useId();
    const invalid = !!error;

    const commit = (text: string) => {
        const added = normalize(text.split(SPLIT_RE), value);
        if (added.length) onChange([...value, ...added]);
        setDraft('');
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter' || e.key === ',' || e.key === '，') {
            if (draft.trim()) {
                e.preventDefault();
                commit(draft);
            } else if (e.key !== 'Enter') {
                e.preventDefault();
            }
            return;
        }
        if (e.key === 'Backspace' && !draft && value.length) {
            e.preventDefault();
            onChange(value.slice(0, -1));
        }
    };

    const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
        const text = e.clipboardData.getData('text');
        if (!SPLIT_RE.test(text.trim())) return;
        e.preventDefault();
        commit(`${draft} ${text}`);
    };

    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            {label && (
                <label htmlFor={id} className="text-xs font-medium text-text-secondary">
                    {label}
                </label>
            )}
            <div
                className={cn(
                    'flex min-h-9 w-full flex-wrap items-center gap-1.5 rounded-sm border bg-field px-2 py-1.5',
                    'transition-colors duration-150 focus-within:ring-2 focus-within:ring-inset',
                    invalid
                        ? 'border-danger focus-within:ring-danger'
                        : 'border-border-subtle focus-within:border-brand focus-within:ring-brand',
                    disabled && 'cursor-not-allowed bg-inset opacity-70',
                )}
                onClick={() => inputRef.current?.focus()}
            >
                {value.map((item) => (
                    <span
                        key={item}
                        className={cn(
                            'inline-flex h-6 max-w-full items-center gap-1 rounded-pill bg-inset pl-2 pr-1 text-xs text-text',
                            mono && 'font-mono',
                        )}
                    >
                        <span className="truncate">{item}</span>
                        {!disabled && (
                            <button
                                type="button"
                                aria-label={`移除 ${item}`}
                                className="rounded-full p-0.5 text-text-tertiary hover:bg-border-subtle hover:text-text"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onChange(value.filter((v) => v !== item));
                                }}
                            >
                                <X size={11} />
                            </button>
                        )}
                    </span>
                ))}
                <input
                    ref={inputRef}
                    id={id}
                    value={draft}
                    disabled={disabled}
                    placeholder={value.length ? '' : placeholder}
                    aria-invalid={invalid || undefined}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={onKeyDown}
                    onPaste={onPaste}
                    onBlur={() => draft.trim() && commit(draft)}
                    className={cn(
                        'min-w-[8rem] flex-1 bg-transparent text-sm text-text outline-none placeholder:text-text-tertiary',
                        mono && 'font-mono',
                    )}
                />
            </div>
            {(hint || error) && (
                <p className={cn('text-2xs leading-snug', invalid ? 'text-danger' : 'text-text-tertiary')}>
                    {error ?? hint}
                </p>
            )}
        </div>
    );
};
