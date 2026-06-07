// 文本输入字段。第二轮:聚焦时给 input 加一次 ring pop 反馈,失焦回静。
//
// CSS focus 状态本身已经是 ring + border 切换,新增 GSAP 在 focus 瞬间做一次
// scale 1.0 → popPeak → 1.0 的轻 pulse(rich/standard 档),给"光标进来了"一个
// 视觉确认。elegant 档不动。
//
// 错误态(invalid=true)出现时如果是新出现的错误,触发 m.shake 一次,提示用户
// 有问题。靠 prevError ref 判断"刚出现"还是"持续中",避免每次 render 都 shake。

import {
    forwardRef,
    useEffect,
    useId,
    useImperativeHandle,
    useRef,
    type InputHTMLAttributes,
    type ReactNode,
} from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

const inputVariants = cva(
    [
        'block w-full rounded-sm bg-field px-3 py-2 text-sm text-text',
        'border outline-none transition-colors duration-150',
        'placeholder:text-text-tertiary',
        'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
        'focus:ring-2 focus:ring-brand focus:ring-offset-2 focus:ring-offset-canvas',
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
    /** 字段下灰色辅助说明,被 error 覆盖。 */
    hint?: ReactNode;
    /** 校验错误文本。出现时边框变红。 */
    error?: ReactNode;
    required?: boolean;
    /** 字符串受控回调。 */
    onValueChange?: (value: string) => void;
}

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(
    (
        { label, hint, error, required, onValueChange, className, id, ...rest },
        ref,
    ) => {
        const invalid = !!error;
        // useId fallback:调用方没给 id/name 时也保证 label↔input↔描述能关联,
        // 满足可访问名称要求(屏幕阅读器、自动化测试都依赖这层关联)。
        const generatedId = useId();
        const fieldId = id ?? rest.name ?? generatedId;
        const describedById = fieldId ? `${fieldId}-desc` : undefined;
        const m = useMotion();
        const inputRef = useRef<HTMLInputElement | null>(null);
        useImperativeHandle(ref, () => inputRef.current!, []);
        const prevErrorRef = useRef<ReactNode>(undefined);

        // 聚焦反馈:focus 时给 input 一次轻 pop。
        useEffect(() => {
            const el = inputRef.current;
            if (!el) return;
            const onFocus = () => {
                if (!m.enabled) return;
                m.pop(el, { peak: 1 + (m.preset.feel.popPeak - 1) * 0.3, ease: 'enterMicro' });
            };
            el.addEventListener('focus', onFocus);
            return () => el.removeEventListener('focus', onFocus);
        }, [m.enabled, m.level, m.speed, m.pop]);

        // 错误首次出现时 shake。
        useEffect(() => {
            const el = inputRef.current;
            if (!el) return;
            const hadError = !!prevErrorRef.current;
            const hasError = !!error;
            if (!hadError && hasError) {
                m.shake(el);
            }
            prevErrorRef.current = error;
        }, [error, m.enabled, m.level, m.speed, m.shake]);

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
                    ref={inputRef}
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
