// 数值输入字段。
//
// 比 TextField type="number" 多两层处理：
//   - 空字符串 → onValueChange(null)，让调用方决定要不要 fallback 到 0
//   - 非数字 / 解析失败 → 不触发 onValueChange，保留旧值视图（避免 Number("abc") = NaN）
//
// 不画 spinner（默认浏览器 spinner 在等宽窄输入框里太挤），如需步进按钮另写一个组件。

import { forwardRef, type InputHTMLAttributes, type ReactNode } from 'react';
import { cn } from '../utils/cn';

const baseClass = [
    'block w-full rounded-sm bg-field px-3 py-2 text-sm text-text',
    'border outline-none transition-colors duration-150',
    'placeholder:text-text-tertiary',
    'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
    'focus:ring-1 focus:ring-brand',
    'tabular-nums',
    // 隐藏浏览器默认 spinner —— 桌面应用里更整洁
    '[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none',
].join(' ');

export interface NumberFieldProps
    extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value' | 'type' | 'size'> {
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    required?: boolean;
    /**
     * 空时回调 null；非数字时不触发回调（保持旧值）。
     * 如果你需要"非法输入也回写 NaN/0" 自己外面 wrap 一下。
     */
    onValueChange?: (value: number | null) => void;
    value: number | null | undefined;
    /** 是否允许小数。默认 false（端口、PID、心跳间隔都是整数）。 */
    allowFloat?: boolean;
}

export const NumberField = forwardRef<HTMLInputElement, NumberFieldProps>(
    (
        {
            label,
            hint,
            error,
            required,
            onValueChange,
            value,
            allowFloat = false,
            className,
            id,
            ...rest
        },
        ref,
    ) => {
        const invalid = !!error;
        const fieldId = id ?? rest.name;
        const describedById = fieldId ? `${fieldId}-desc` : undefined;

        return (
            <div className={cn('flex flex-col gap-1.5', className)}>
                {label && (
                    <label
                        htmlFor={fieldId}
                        className="text-xs font-medium text-text-secondary"
                    >
                        {label}
                        {required && <span className="ml-0.5 text-danger">*</span>}
                    </label>
                )}
                <input
                    ref={ref}
                    id={fieldId}
                    type="number"
                    inputMode={allowFloat ? 'decimal' : 'numeric'}
                    aria-invalid={invalid || undefined}
                    aria-describedby={describedById}
                    value={value ?? ''}
                    className={cn(
                        baseClass,
                        invalid
                            ? 'border-danger focus:border-danger focus:ring-danger'
                            : 'border-border-subtle focus:border-brand',
                    )}
                    onChange={(e) => {
                        const raw = e.target.value;
                        if (raw === '') {
                            onValueChange?.(null);
                            return;
                        }
                        const n = allowFloat ? parseFloat(raw) : parseInt(raw, 10);
                        if (Number.isFinite(n)) onValueChange?.(n);
                    }}
                    {...rest}
                />
                {(hint || error) && (
                    <p
                        id={describedById}
                        className={cn(
                            'text-2xs leading-snug',
                            invalid ? 'text-danger' : 'text-text-tertiary',
                        )}
                    >
                        {error ?? hint}
                    </p>
                )}
            </div>
        );
    },
);
NumberField.displayName = 'NumberField';
