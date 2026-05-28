// 文本输入字段。Label + Input + hint/error 一体化。
//
// 参考表单 UX：标签上置、内联校验、必填带 *、错误状态边框变红、focus 状态描边
// 走 brand。受控形态对外暴露 value / onValueChange，内部把 onChange 翻译成纯字符串
// 回调，少写样板。
//
// 不接 leftIcon / rightIcon —— 真要带前缀（http://）走 children slot。

import { forwardRef, type InputHTMLAttributes, type ReactNode } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const inputVariants = cva(
    [
        'block w-full rounded-sm bg-field px-3 py-2 text-sm text-text',
        'border outline-none transition-colors duration-150',
        'placeholder:text-text-tertiary',
        'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
        'focus:ring-1 focus:ring-brand',
    ],
    {
        variants: {
            invalid: {
                false: 'border-border-subtle focus:border-brand',
                true: 'border-danger focus:border-danger focus:ring-danger',
            },
        },
        defaultVariants: { invalid: false },
    },
);

export interface TextFieldProps
    extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'size'>,
    VariantProps<typeof inputVariants> {
    label?: ReactNode;
    /** 字段下灰色辅助说明，被 error 覆盖。 */
    hint?: ReactNode;
    /** 校验错误文本。出现时边框变红。 */
    error?: ReactNode;
    required?: boolean;
    /** 字符串受控回调，避免每个调用方再 e.target.value。 */
    onValueChange?: (value: string) => void;
}

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(
    (
        { label, hint, error, required, onValueChange, className, id, ...rest },
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
                    aria-invalid={invalid || undefined}
                    aria-describedby={describedById}
                    className={inputVariants({ invalid })}
                    onChange={(e) => {
                        onValueChange?.(e.target.value);
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
TextField.displayName = 'TextField';
