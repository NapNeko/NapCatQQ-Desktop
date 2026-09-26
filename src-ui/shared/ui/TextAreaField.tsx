// 多行文本字段：外观、label / hint / error 的排法和 TextField 一致，给人格、提示词这类长文用。
// 高度随内容撑开到 maxRows 为止，再多就在框里滚，免得一大段提示词把整页推得很长。

import { useEffect, useId, useLayoutEffect, useRef, type ReactNode, type TextareaHTMLAttributes } from 'react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export interface TextAreaFieldProps
    extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, 'onChange' | 'value' | 'rows'> {
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    required?: boolean;
    value: string;
    onValueChange?: (value: string) => void;
    /** 最少显示几行，默认 3 */
    minRows?: number;
    /** 撑到几行后改为框内滚动，默认 12 */
    maxRows?: number;
    mono?: boolean;
}

const LINE_HEIGHT_PX = 20;
const PADDING_Y_PX = 16;

export const TextAreaField: React.FC<TextAreaFieldProps> = ({
    label,
    hint,
    error,
    required,
    value,
    onValueChange,
    minRows = 3,
    maxRows = 12,
    mono,
    className,
    id,
    ...rest
}) => {
    const invalid = !!error;
    const generatedId = useId();
    const fieldId = id ?? rest.name ?? generatedId;
    const describedById = `${fieldId}-desc`;
    const ref = useRef<HTMLTextAreaElement | null>(null);
    const m = useMotion();
    const prevErrorRef = useRef<ReactNode>(undefined);

    // 先收回到最小再量 scrollHeight，删字时才缩得回去
    useLayoutEffect(() => {
        const el = ref.current;
        if (!el) return;
        const min = minRows * LINE_HEIGHT_PX + PADDING_Y_PX;
        const max = maxRows * LINE_HEIGHT_PX + PADDING_Y_PX;
        el.style.height = `${min}px`;
        el.style.height = `${Math.min(Math.max(el.scrollHeight, min), max)}px`;
    }, [value, minRows, maxRows]);

    useEffect(() => {
        const el = ref.current;
        if (el && !prevErrorRef.current && error) m.shake(el);
        prevErrorRef.current = error;
    }, [error, m]);

    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            {label && (
                <label htmlFor={fieldId} className="text-xs font-medium text-text-secondary">
                    {label}
                    {required && <span className="ml-0.5 text-danger">*</span>}
                </label>
            )}
            <textarea
                ref={ref}
                id={fieldId}
                value={value}
                aria-invalid={invalid || undefined}
                aria-describedby={hint || error ? describedById : undefined}
                onChange={(e) => onValueChange?.(e.target.value)}
                className={cn(
                    'block w-full resize-none rounded-sm border bg-field px-3 py-2 text-sm leading-5 text-text',
                    'outline-none transition-colors duration-150 placeholder:text-text-tertiary',
                    'focus:ring-2 focus:ring-inset',
                    'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
                    invalid
                        ? 'border-danger focus:border-danger focus:ring-danger'
                        : 'border-border-subtle focus:border-brand focus:ring-brand',
                    mono && 'font-mono',
                )}
                {...rest}
            />
            {(hint || error) && (
                <p
                    id={describedById}
                    className={cn('text-2xs leading-snug', invalid ? 'text-danger' : 'text-text-tertiary')}
                >
                    {error ?? hint}
                </p>
            )}
        </div>
    );
};
