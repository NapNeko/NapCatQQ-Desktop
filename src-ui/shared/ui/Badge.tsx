// 状态徽章原子件。NapCat 业务里同一张 BotCard 经常并排挂 3-5 个徽章
// （bot state + flavor + pending_restart + login_state），所以 Badge 必须紧凑、
// 高度 20px、字号 11-12px，不能用 Fluent 那种偏胖的默认尺寸。
//
// 设计规则：
//   - tone: 决定颜色语义（neutral / brand / success / warning / danger / info）
//   - appearance: 决定填充强度（soft 浅底深字 / solid 深底白字 / outline 边框）
//   - 内部留 dot 槽位：tone === 'success' 且 appearance === 'soft' 时自动加发光圆点。
//     这是 "在线" 徽章的标志性视觉，复用率极高，提到原子件层。

import { forwardRef, type HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const badgeVariants = cva(
    'inline-flex items-center gap-1.5 rounded-pill px-2 py-0.5 text-2xs font-medium leading-none whitespace-nowrap',
    {
        variants: {
            tone: {
                neutral: '',
                brand: '',
                success: '',
                warning: '',
                danger: '',
                info: '',
            },
            appearance: {
                soft: '',
                solid: '',
                outline: 'bg-transparent',
            },
        },
        compoundVariants: [
            // soft：浅底深字
            { tone: 'neutral', appearance: 'soft', className: 'bg-inset text-text-secondary' },
            { tone: 'brand', appearance: 'soft', className: 'bg-brand-soft text-brand' },
            { tone: 'success', appearance: 'soft', className: 'bg-success-soft text-success' },
            { tone: 'warning', appearance: 'soft', className: 'bg-warning-soft text-warning' },
            { tone: 'danger', appearance: 'soft', className: 'bg-danger-soft text-danger' },
            { tone: 'info', appearance: 'soft', className: 'bg-info-soft text-info' },
            // solid：深底白字
            { tone: 'neutral', appearance: 'solid', className: 'bg-text-secondary text-white' },
            { tone: 'brand', appearance: 'solid', className: 'bg-brand text-white' },
            { tone: 'success', appearance: 'solid', className: 'bg-success text-white' },
            { tone: 'warning', appearance: 'solid', className: 'bg-warning text-white' },
            { tone: 'danger', appearance: 'solid', className: 'bg-danger text-white' },
            { tone: 'info', appearance: 'solid', className: 'bg-info text-white' },
            // outline：边框 + 同色字
            { tone: 'neutral', appearance: 'outline', className: 'border border-border text-text-secondary' },
            { tone: 'brand', appearance: 'outline', className: 'border border-brand text-brand' },
            { tone: 'success', appearance: 'outline', className: 'border border-success text-success' },
            { tone: 'warning', appearance: 'outline', className: 'border border-warning text-warning' },
            { tone: 'danger', appearance: 'outline', className: 'border border-danger text-danger' },
            { tone: 'info', appearance: 'outline', className: 'border border-info text-info' },
        ],
        defaultVariants: {
            tone: 'neutral',
            appearance: 'soft',
        },
    },
);

export interface BadgeProps
    extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {
    /** 徽章前的指示圆点。常用：success + dot 表示 Bot online。 */
    dot?: boolean;
}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
    ({ className, tone, appearance, dot, children, ...props }, ref) => (
        <span ref={ref} className={cn(badgeVariants({ tone, appearance }), className)} {...props}>
            {dot && <BadgeDot tone={tone ?? 'neutral'} />}
            {children}
        </span>
    ),
);
Badge.displayName = 'Badge';

// dot 单独抽出来供卡片以外的地方直接用（比如 sidebar nav item 的未读小点）。

const dotVariants = cva('inline-block h-1.5 w-1.5 rounded-full', {
    variants: {
        tone: {
            neutral: 'bg-text-tertiary',
            brand: 'bg-brand',
            success: 'bg-success shadow-glow-success',
            warning: 'bg-warning',
            danger: 'bg-danger',
            info: 'bg-info',
        },
    },
    defaultVariants: { tone: 'neutral' },
});

export const BadgeDot: React.FC<{ tone: NonNullable<BadgeProps['tone']>; className?: string }> = ({
    tone,
    className,
}) => <span className={cn(dotVariants({ tone }), className)} />;
