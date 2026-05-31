// 加载指示器原子件。纯 CSS 实现，尺寸与 Button 高度对齐。
// 主要消费方：Button 加载态（Button 内 children 自由排版，所以不内置 loading prop）；
// 列表 / 配置页 fetch loading 时居中展示。

import { forwardRef, type HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const spinnerVariants = cva(
    'inline-block animate-spin rounded-full border-2 border-current border-r-transparent',
    {
        variants: {
            size: {
                xs: 'h-3 w-3 border',
                sm: 'h-4 w-4',
                md: 'h-5 w-5',
                lg: 'h-8 w-8 border-[3px]',
            },
            tone: {
                default: 'text-text-tertiary',
                brand: 'text-brand',
                muted: 'text-text-disabled',
            },
        },
        defaultVariants: { size: 'md', tone: 'default' },
    },
);

export interface SpinnerProps
    extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof spinnerVariants> {
    /** 屏幕阅读器友好。提供时会渲染 sr-only 文本。 */
    label?: string;
}

export const Spinner = forwardRef<HTMLSpanElement, SpinnerProps>(
    ({ className, size, tone, label = '加载中', ...props }, ref) => (
        <span
            ref={ref}
            role="status"
            aria-live="polite"
            className={cn(spinnerVariants({ size, tone }), className)}
            {...props}
        >
            <span className="sr-only">{label}</span>
        </span>
    ),
);
Spinner.displayName = 'Spinner';
