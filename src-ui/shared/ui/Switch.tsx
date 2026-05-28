// Switch 原子件。语义：开关而不是勾选。比 Checkbox 视觉更明确，用于
// "启用此连接 / 启用 O3 Hook 注入 / 开机自启" 这类二元状态。
//
// 默认尺寸 28×16，比常见 Switch 小一档，匹配密集表单的紧凑感。

import * as RadixSwitch from '@radix-ui/react-switch';
import { forwardRef, type ReactNode } from 'react';
import { cn } from '../utils/cn';

export interface SwitchProps
    extends Omit<React.ComponentPropsWithoutRef<typeof RadixSwitch.Root>, 'onCheckedChange'> {
    label?: ReactNode;
    hint?: ReactNode;
    onCheckedChange?: (checked: boolean) => void;
}

export const Switch = forwardRef<React.ElementRef<typeof RadixSwitch.Root>, SwitchProps>(
    ({ label, hint, onCheckedChange, className, id, disabled, ...rest }, ref) => {
        const fieldId = id ?? rest.name;
        const sw = (
            <RadixSwitch.Root
                ref={ref}
                id={fieldId}
                disabled={disabled}
                onCheckedChange={onCheckedChange}
                className={cn(
                    'relative inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-pill',
                    'border border-border-strong bg-inset transition-colors',
                    'data-[state=checked]:border-brand data-[state=checked]:bg-brand',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                    'disabled:cursor-not-allowed disabled:opacity-50',
                )}
                {...rest}
            >
                <RadixSwitch.Thumb className={cn(
                    'block h-3 w-3 translate-x-0.5 rounded-full bg-white shadow-sm transition-transform',
                    'data-[state=checked]:translate-x-[14px]',
                )} />
            </RadixSwitch.Root>
        );

        if (!label) return <div className={className}>{sw}</div>;

        return (
            <label
                htmlFor={fieldId}
                className={cn(
                    'inline-flex items-start gap-2.5 cursor-pointer',
                    disabled && 'cursor-not-allowed opacity-60',
                    className,
                )}
            >
                {sw}
                <span className="flex flex-col gap-0.5 leading-tight">
                    <span className="text-sm text-text">{label}</span>
                    {hint && <span className="text-2xs text-text-tertiary">{hint}</span>}
                </span>
            </label>
        );
    },
);
Switch.displayName = 'Switch';
