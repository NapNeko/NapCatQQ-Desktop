// 进度条原子件。
//
// 设计目标：
//   - 安装 / 下载 / 切镜像三种语义共用一个组件，由 indeterminate / tone 区分
//   - 可选 size，default md (4px 高)，sm (3px) 给紧凑列表
//   - indeterminate 模式：暖色 thumb 在轨道上来回滑（race / 切镜像阶段，没有可
//     靠百分比可显）。靠 keyframe 名 `progress-indeterminate` 生成，定义在
//     app/index.css 的 @theme + @keyframes
//   - determinate 模式：thumb 宽 = percent%，过渡 250ms ease-out 让数字跳动平滑
//
// 不在本组件做的事：
//   - 数字 / 速度 / ETA 文本：调用方自己用 formatBytes / formatSpeed 拼，因为
//     不同位置（HostStatusRow 单行、ComponentCard 详情）排版差别大
//   - aria-* 自动维护：value 和 max 透传给消费者，indeterminate 时 role 给 progressbar 但 不给 valuenow

import { forwardRef, type HTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';

const trackVariants = cva(
    'relative w-full overflow-hidden rounded-pill bg-inset',
    {
        variants: {
            size: {
                sm: 'h-[3px]',
                md: 'h-[5px]',
                lg: 'h-2',
            },
        },
        defaultVariants: { size: 'md' },
    },
);

const thumbVariants = cva(
    'absolute inset-y-0 left-0 rounded-pill transition-[width,background-color] duration-300 ease-out',
    {
        variants: {
            tone: {
                brand: 'bg-brand',
                success: 'bg-success',
                warning: 'bg-warning',
                danger: 'bg-danger',
            },
        },
        defaultVariants: { tone: 'brand' },
    },
);

const indeterminateThumbVariants = cva(
    'absolute inset-y-0 left-0 w-[35%] rounded-pill animate-progress-indeterminate',
    {
        variants: {
            tone: {
                brand: 'bg-brand',
                success: 'bg-success',
                warning: 'bg-warning',
                danger: 'bg-danger',
            },
        },
        defaultVariants: { tone: 'brand' },
    },
);

export interface ProgressProps
    extends Omit<HTMLAttributes<HTMLDivElement>, 'role'>,
    VariantProps<typeof trackVariants> {
    /** 0-100。indeterminate=true 时忽略。 */
    value?: number;
    /** 无确定进度。常用于 race / 切镜像阶段。 */
    indeterminate?: boolean;
    /** 颜色语义。下载 / 安装走 brand；warning 给"切镜像中"用更醒目的态度。 */
    tone?: 'brand' | 'success' | 'warning' | 'danger';
}

export const Progress = forwardRef<HTMLDivElement, ProgressProps>(
    ({ className, size, value = 0, indeterminate, tone = 'brand', ...props }, ref) => {
        const clamped = Math.max(0, Math.min(100, value));
        return (
            <div
                ref={ref}
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={indeterminate ? undefined : Math.round(clamped)}
                aria-busy={indeterminate ? true : undefined}
                className={cn(trackVariants({ size }), className)}
                {...props}
            >
                {indeterminate ? (
                    <span className={indeterminateThumbVariants({ tone })} />
                ) : (
                    <span
                        className={thumbVariants({ tone })}
                        style={{ width: `${clamped}%` }}
                    />
                )}
            </div>
        );
    },
);
Progress.displayName = 'Progress';
