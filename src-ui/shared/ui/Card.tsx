// 卡片原子件。三种 variant 对应参考图 home 页三种卡：
//   - default：白底默认卡（version / occupancy / 单 bot 卡）
//   - hero：暖米黄 hello 卡，圆角更大、padding 更宽，配 mascot 浮挂
//   - inset：嵌入式段落（卡内的 sub-section，比如 hello 卡里的 success 提示行）
//
// Card 不接 onClick / 也不做受控；clickable 由组件外层包 <button> 决定，避免把交互语义糊到展示件里。

import { forwardRef, type HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const cardVariants = cva(
    'rounded-md transition-shadow duration-200',
    {
        variants: {
            variant: {
                default: 'bg-surface shadow-card',
                hero: 'rounded-lg shadow-card border border-border-subtle bg-[var(--surface-hero)]',
                inset: 'bg-inset',
                outlined: 'bg-surface border border-border-subtle',
                ghost: 'bg-transparent',
            },
            padding: {
                none: 'p-0',
                sm: 'p-3',
                md: 'p-5',
                lg: 'p-6',
                xl: 'p-8',
            },
            hover: {
                none: '',
                lift: 'hover:shadow-popover hover:-translate-y-px',
            },
        },
        defaultVariants: {
            variant: 'default',
            padding: 'md',
            hover: 'none',
        },
    },
);

export interface CardProps
    extends HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof cardVariants> { }

export const Card = forwardRef<HTMLDivElement, CardProps>(
    ({ className, variant, padding, hover, ...props }, ref) => (
        <div
            ref={ref}
            className={cn(cardVariants({ variant, padding, hover }), className)}
            {...props}
        />
    ),
);
Card.displayName = 'Card';

// 子组件：让外部以语义化方式拼装卡片头/尾，padding 已由 Card 控制，子组件只管排版。

export const CardHeader = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
    ({ className, ...props }, ref) => (
        <div
            ref={ref}
            className={cn('mb-3 flex items-start justify-between gap-3', className)}
            {...props}
        />
    ),
);
CardHeader.displayName = 'CardHeader';

export const CardTitle = forwardRef<HTMLHeadingElement, HTMLAttributes<HTMLHeadingElement>>(
    ({ className, ...props }, ref) => (
        <h3
            ref={ref}
            className={cn('font-display text-md font-semibold text-text', className)}
            {...props}
        />
    ),
);
CardTitle.displayName = 'CardTitle';

export const CardDescription = forwardRef<HTMLParagraphElement, HTMLAttributes<HTMLParagraphElement>>(
    ({ className, ...props }, ref) => (
        <p
            ref={ref}
            className={cn('text-xs text-text-tertiary', className)}
            {...props}
        />
    ),
);
CardDescription.displayName = 'CardDescription';

export const CardFooter = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
    ({ className, ...props }, ref) => (
        <div
            ref={ref}
            className={cn('mt-4 flex items-center justify-end gap-2', className)}
            {...props}
        />
    ),
);
CardFooter.displayName = 'CardFooter';
