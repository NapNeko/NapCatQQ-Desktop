import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';

export function IdentityChoiceCard({
    selected,
    disabled,
    onSelect,
    label,
    summary,
    icon: Icon,
    badge,
}: {
    selected: boolean;
    disabled?: boolean;
    onSelect: () => void;
    label: string;
    summary: string;
    icon?: LucideIcon;
    badge?: ReactNode;
}) {
    return (
        <button
            type="button"
            role="radio"
            aria-checked={selected}
            disabled={disabled}
            onClick={onSelect}
            className={cn(
                'group relative flex h-full min-w-0 rounded-md border px-3.5 py-2.5 text-left transition-all',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-60',
                Icon ? 'items-center justify-between gap-3' : 'flex-col justify-between gap-1',
                selected
                    ? 'border-brand/70 bg-brand-soft/20 shadow-xs ring-1 ring-brand/25'
                    : 'border-border-subtle bg-field/45 hover:border-border hover:bg-field/80',
            )}
        >
            {Icon ? (
                <>
                    <div className="flex min-w-0 items-center gap-2.5">
                        <span
                            className={cn(
                                'flex h-8 w-8 shrink-0 items-center justify-center rounded-sm border transition-colors',
                                selected
                                    ? 'border-brand/30 bg-brand-soft text-brand'
                                    : 'border-border-subtle bg-inset text-text-secondary',
                            )}
                        >
                            <Icon size={15} strokeWidth={2.1} />
                        </span>
                        <div className="min-w-0">
                            <div className="text-sm font-medium text-text">{label}</div>
                            <div className="truncate text-2xs text-text-tertiary">{summary}</div>
                        </div>
                    </div>
                    <SelectionDot selected={selected} />
                </>
            ) : (
                <>
                    <div className="flex w-full items-center justify-between gap-2">
                        <div className="flex min-w-0 items-center gap-2">
                            <span className="truncate text-sm font-semibold text-text">
                                {label}
                            </span>
                            {badge}
                        </div>
                        <SelectionDot selected={selected} />
                    </div>
                    <p className="text-2xs leading-relaxed text-text-tertiary">{summary}</p>
                </>
            )}
        </button>
    );
}

function SelectionDot({ selected }: { selected: boolean }) {
    return (
        <span
            aria-hidden
            className={cn(
                'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors',
                selected ? 'border-brand bg-brand/10' : 'border-border-strong bg-field',
            )}
        >
            <span
                className={cn(
                    'h-2 w-2 rounded-full bg-brand transition-transform duration-150',
                    selected ? 'scale-100 opacity-100' : 'scale-50 opacity-0',
                )}
            />
        </span>
    );
}
