// Checkbox 原子件。Radix 兜底 a11y / 焦点环 / 键盘空格切换。
//
// 设计：
//   - 自带 label 平铺右侧（多数业务用法），不用 label 时直接渲 box
//   - hint 字段放在 label 下，给"开关项的副说明"留位置
//   - 不做 indeterminate 状态（NapCat 业务没用上）

import * as RadixCheckbox from '@radix-ui/react-checkbox';
import { Check } from 'lucide-react';
import { forwardRef, type ReactNode } from 'react';
import { cn } from '../utils/cn';

export interface CheckboxProps
    extends Omit<React.ComponentPropsWithoutRef<typeof RadixCheckbox.Root>, 'onCheckedChange'> {
    label?: ReactNode;
    hint?: ReactNode;
    onCheckedChange?: (checked: boolean) => void;
}

export const Checkbox = forwardRef<
    React.ElementRef<typeof RadixCheckbox.Root>,
    CheckboxProps
>(({ label, hint, onCheckedChange, className, id, disabled, ...rest }, ref) => {
    const fieldId = id ?? rest.name;
    const box = (
        <RadixCheckbox.Root
            ref={ref}
            id={fieldId}
            disabled={disabled}
            onCheckedChange={(c) => onCheckedChange?.(c === true)}
            className={cn(
                'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-xs',
                'border border-border-strong bg-field transition-colors',
                'data-[state=checked]:border-brand data-[state=checked]:bg-brand',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                'disabled:cursor-not-allowed disabled:opacity-50',
            )}
            {...rest}
        >
            <RadixCheckbox.Indicator className="flex items-center justify-center text-white">
                <Check size={11} strokeWidth={3} />
            </RadixCheckbox.Indicator>
        </RadixCheckbox.Root>
    );

    if (!label) return <div className={className}>{box}</div>;

    return (
        <label
            htmlFor={fieldId}
            className={cn(
                'inline-flex items-start gap-2 cursor-pointer',
                disabled && 'cursor-not-allowed opacity-60',
                className,
            )}
        >
            {box}
            <span className="flex flex-col gap-0.5 leading-tight">
                <span className="text-sm text-text">{label}</span>
                {hint && <span className="text-2xs text-text-tertiary">{hint}</span>}
            </span>
        </label>
    );
});
Checkbox.displayName = 'Checkbox';
