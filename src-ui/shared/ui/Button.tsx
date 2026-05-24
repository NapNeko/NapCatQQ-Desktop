// 通用按钮原子件。shadcn 模式：cva 定义 variant + size，class-variance-authority 编译时生成稳定类名。
//
// 设计取舍：
//   - 不用 Radix Slot 的 asChild 模式（避免后续重写时把 <Button asChild><a>... 这种隐式语义带进来）。
//   - 仅 4 个 variant：primary / secondary / ghost / danger。NapCat 业务里没必要更多。
//   - icon 通过 children 自由排版，不像 Fluent 那样接 leftIcon / rightIcon prop，给页面更多控制权。

import { forwardRef, type ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const buttonVariants = cva(
    // 基础：所有 variant 共用。focus ring 用 brand 描边而不是 box-shadow，避免和卡片阴影叠加发糊。
    'inline-flex items-center justify-center gap-2 rounded-sm font-medium ' +
    'transition-colors duration-150 ease-out ' +
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-canvas ' +
    'disabled:pointer-events-none disabled:opacity-50 select-none',
    {
        variants: {
            variant: {
                primary:
                    'bg-brand text-white shadow-card hover:bg-brand-hover active:translate-y-[1px]',
                secondary:
                    'bg-surface text-text border border-border-subtle hover:bg-inset hover:border-border',
                ghost:
                    'bg-transparent text-text-secondary hover:bg-inset hover:text-text',
                danger:
                    'bg-danger text-white shadow-card hover:opacity-90 active:translate-y-[1px]',
            },
            size: {
                sm: 'h-7 px-2.5 text-xs',
                md: 'h-9 px-3.5 text-sm',
                lg: 'h-11 px-5 text-base',
                icon: 'h-9 w-9 p-0',
            },
        },
        defaultVariants: {
            variant: 'secondary',
            size: 'md',
        },
    },
);

export interface ButtonProps
    extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> { }

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
    ({ className, variant, size, type = 'button', ...props }, ref) => (
        <button
            ref={ref}
            type={type}
            className={cn(buttonVariants({ variant, size }), className)}
            {...props}
        />
    ),
);
Button.displayName = 'Button';
