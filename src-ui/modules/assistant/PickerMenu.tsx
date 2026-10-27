// 输入框工具栏上的紧凑选择器：目标 Bot、助手。

import type { LucideIcon } from 'lucide-react';
import { ChevronDown, Loader2 } from 'lucide-react';
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuRadioGroup,
    DropdownMenuRadioItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from '../../shared/ui';

export interface PickerOption<V extends string> {
    value: V;
    label: string;
    detail?: string;
    disabled?: boolean;
}

interface Props<V extends string> {
    label: string;
    icon: LucideIcon;
    value: V;
    text: string;
    options: PickerOption<V>[];
    loading?: string | null;
    action?: { label: string; onSelect: () => void };
    disabled?: boolean;
    onChange: (value: V) => void;
    onOpenChange?: (open: boolean) => void;
}

export function PickerMenu<V extends string>({
    label,
    icon: Icon,
    value,
    text,
    options,
    loading,
    action,
    disabled,
    onChange,
    onOpenChange,
}: Props<V>) {
    return (
        <DropdownMenu onOpenChange={onOpenChange}>
            <DropdownMenuTrigger
                disabled={disabled}
                aria-label={`${label}：${text}`}
                title={text}
                className="inline-flex h-7 min-w-0 max-w-[15rem] items-center gap-1.5 rounded-pill px-2.5 text-xs text-text-secondary outline-none transition-colors hover:bg-inset hover:text-text focus-visible:ring-2 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50 data-[state=open]:bg-inset data-[state=open]:text-text"
            >
                <Icon size={13} className="shrink-0" aria-hidden />
                <span className="min-w-0 truncate">{text}</span>
                <ChevronDown size={12} className="shrink-0 text-text-tertiary" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64 max-w-[calc(100vw-2rem)]">
                <DropdownMenuLabel>{label}</DropdownMenuLabel>
                <DropdownMenuRadioGroup value={value} onValueChange={(next) => onChange(next as V)}>
                    {options.map((option) => (
                        <DropdownMenuRadioItem
                            key={option.value}
                            value={option.value}
                            disabled={option.disabled}
                            title={[option.label, option.detail].filter(Boolean).join(' · ')}
                        >
                            <span className="min-w-0 flex-1 truncate">{option.label}</span>
                            {option.detail && (
                                <span className="max-w-[45%] shrink-0 truncate text-2xs text-text-tertiary">
                                    {option.detail}
                                </span>
                            )}
                        </DropdownMenuRadioItem>
                    ))}
                </DropdownMenuRadioGroup>
                {loading && (
                    <p
                        role="status"
                        className="flex items-center gap-2 px-2.5 py-1.5 text-xs text-text-tertiary"
                    >
                        <Loader2 size={12} className="animate-spin" />
                        {loading}
                    </p>
                )}
                {action && (
                    <>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem onSelect={action.onSelect}>
                            {action.label}
                        </DropdownMenuItem>
                    </>
                )}
            </DropdownMenuContent>
        </DropdownMenu>
    );
}
