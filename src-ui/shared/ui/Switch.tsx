// Switch 原子件。第二轮:thumb 滑动接入 GSAP,跟随档位/速度/reduced。
//
// Radix Switch 用 data-state=checked/unchecked 切换,thumb 是它内部的 span。
// 我们用 useLayoutEffect 监听受控 checked 变化,GSAP 动 thumb 的 x 位置;
// 同时对整个 Root 做一次"check 落位 pop"反馈(scale 1 → popPeak → 1)。
//
// 不重写 thumb 的 CSS class:checked 状态下 Radix 会自动给 thumb 加属性,
// 但我们 override 它的 transition,改用 GSAP transform x。Tailwind 类里
// 的 translate-x-* 移除,避免 CSS / GSAP 同时控同一属性打架。

import * as RadixSwitch from '@radix-ui/react-switch';
import { forwardRef, useLayoutEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export interface SwitchProps
    extends Omit<React.ComponentPropsWithoutRef<typeof RadixSwitch.Root>, 'onCheckedChange' | 'checked'> {
    label?: ReactNode;
    hint?: ReactNode;
    onCheckedChange?: (checked: boolean) => void;
    checked?: boolean;
}

export const Switch = forwardRef<React.ElementRef<typeof RadixSwitch.Root>, SwitchProps>(
    ({ label, hint, onCheckedChange, className, id, disabled, checked, ...rest }, ref) => {
        const m = useMotion();
        const fieldId = id ?? rest.name;
        const rootRef = useRef<HTMLButtonElement | null>(null);
        const thumbRef = useRef<HTMLSpanElement | null>(null);
        const prevCheckedRef = useRef<boolean | undefined>(checked);

        const setRootRef = (node: HTMLButtonElement | null) => {
            rootRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLButtonElement | null>).current = node;
        };

        // thumb 滑动 + 反馈 pop。useLayoutEffect 让首次 mount 也能正确落位。
        useLayoutEffect(() => {
            const root = rootRef.current;
            const thumb = thumbRef.current;
            if (!root || !thumb) return;
            // 几何:track 36px (h-5 w-9), thumb 16px,左右各留 2px。
            // checked 时 thumb 平移 (36 - 16 - 4) = 16px。
            const targetX = checked ? 16 : 0;
            if (!m.enabled) {
                gsap.set(thumb, { x: targetX });
                prevCheckedRef.current = checked;
                return;
            }
            gsap.to(thumb, {
                x: targetX,
                duration: m.duration('fast'),
                ease: m.ease.release,
            });
            // 状态切换时给 root 一次轻 pop,给"刚改完"视觉确认。
            if (prevCheckedRef.current !== undefined && prevCheckedRef.current !== checked) {
                m.pop(root, { peak: 1 + (m.preset.feel.popPeak - 1) * 0.5 });
            }
            prevCheckedRef.current = checked;
        }, [checked, m]);

        const sw = (
            <RadixSwitch.Root
                ref={setRootRef}
                id={fieldId}
                disabled={disabled}
                checked={checked}
                onCheckedChange={onCheckedChange}
                className={cn(
                    'relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-pill',
                    // bg-color 走 CSS transition,thumb 位移走 GSAP,各管一份不打架。
                    'bg-border-strong transition-colors duration-200',
                    'data-[state=checked]:bg-brand',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                    'disabled:cursor-not-allowed disabled:opacity-50',
                )}
                {...rest}
            >
                <RadixSwitch.Thumb asChild>
                    <span
                        ref={thumbRef}
                        className={cn(
                            'block h-4 w-4 rounded-full bg-white shadow-sm',
                        )}
                        // GSAP 用 transform,初始位置由 useLayoutEffect 设置。
                        style={{ marginLeft: 2 }}
                    />
                </RadixSwitch.Thumb>
            </RadixSwitch.Root>
        );

        if (!label) return <div className={className}>{sw}</div>;

        return (
            <label
                htmlFor={fieldId}
                className={cn(
                    'inline-flex items-start gap-2.5 cursor-pointer',
                    disabled && 'cursor-not-allowed opacity-60',
                    className,
                )}
            >
                {sw}
                <span className="flex flex-col gap-0.5 leading-tight">
                    <span className="text-sm text-text">{label}</span>
                    {hint && <span className="text-2xs text-text-tertiary">{hint}</span>}
                </span>
            </label>
        );
    },
);
Switch.displayName = 'Switch';
