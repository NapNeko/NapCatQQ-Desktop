// 进度条原子件。第二轮:thumb 宽度过渡接 GSAP,完成时 pulse 反馈;
// indeterminate 模式仍走 CSS keyframes(那是个无止境循环,GSAP 在 React 卸载/
// 切档位时容易遗留 timeline,反不如纯 CSS 稳)。

import { forwardRef, useLayoutEffect, useRef, type HTMLAttributes } from 'react';
import gsap from 'gsap';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

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
    'absolute inset-y-0 left-0 rounded-pill',
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
    /** 颜色语义。下载 / 安装走 brand;warning 给"切镜像中"用更醒目的态度。 */
    tone?: 'brand' | 'success' | 'warning' | 'danger';
}

export const Progress = forwardRef<HTMLDivElement, ProgressProps>(
    ({ className, size, value = 0, indeterminate, tone = 'brand', ...props }, ref) => {
        const m = useMotion();
        const clamped = Math.max(0, Math.min(100, value));
        const thumbRef = useRef<HTMLSpanElement | null>(null);
        const trackRef = useRef<HTMLDivElement | null>(null);
        const prevValueRef = useRef<number>(clamped);
        const completedRef = useRef<boolean>(clamped >= 100);

        const setTrackRef = (node: HTMLDivElement | null) => {
            trackRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
        };

        useLayoutEffect(() => {
            if (indeterminate) return;
            const thumb = thumbRef.current;
            const track = trackRef.current;
            if (!thumb) return;
            if (!m.enabled) {
                gsap.set(thumb, { width: `${clamped}%` });
                prevValueRef.current = clamped;
                completedRef.current = clamped >= 100;
                return;
            }
            gsap.to(thumb, {
                width: `${clamped}%`,
                duration: m.duration('base'),
                ease: m.ease.hover,
            });
            // 100% 落地时给 track 一个 pulse 反馈(rich/standard 档启用)。
            const justCompleted = clamped >= 100 && !completedRef.current;
            if (justCompleted && track && m.preset.feel.popPeak > 1) {
                m.pop(track, { peak: 1 + (m.preset.feel.popPeak - 1) * 0.4 });
            }
            prevValueRef.current = clamped;
            completedRef.current = clamped >= 100;
        }, [clamped, indeterminate, m]);

        return (
            <div
                ref={setTrackRef}
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
                    <span ref={thumbRef} className={thumbVariants({ tone })} />
                )}
            </div>
        );
    },
);
Progress.displayName = 'Progress';
