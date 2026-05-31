// Checkbox 原子件。第二轮:勾选 indicator 进退场接 GSAP pop,跟主题档位走。
//
// Radix Checkbox.Indicator 在 unchecked 时根本不渲染(它是 conditional)。这导致
// 我们想做"打勾时 indicator pop 进场"的话,GSAP 没有 element 可挂。解决:
//   - 用 forceMount + 自己控 visibility:用 useState 跟踪 checked,checked=true
//     时让 indicator 的 GSAP 跑 from 0.4 scale + opacity 0 → 1 + 1。
//   - unchecked 时反向跑;exit 完后 GSAP 把它隐起来。
//
// elegant 档退化为单纯瞬时显隐(GSAP set + 不跑 tween),保证视觉一致。

import * as RadixCheckbox from '@radix-ui/react-checkbox';
import { Check } from 'lucide-react';
import { forwardRef, useLayoutEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export interface CheckboxProps
    extends Omit<React.ComponentPropsWithoutRef<typeof RadixCheckbox.Root>, 'onCheckedChange' | 'checked'> {
    label?: ReactNode;
    hint?: ReactNode;
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
}

export const Checkbox = forwardRef<
    React.ElementRef<typeof RadixCheckbox.Root>,
    CheckboxProps
>(({ label, hint, onCheckedChange, className, id, disabled, checked, ...rest }, ref) => {
    const m = useMotion();
    const fieldId = id ?? rest.name;
    const rootRef = useRef<HTMLButtonElement | null>(null);
    const indicatorRef = useRef<HTMLSpanElement | null>(null);
    const prevCheckedRef = useRef<boolean | undefined>(checked);

    const setRootRef = (node: HTMLButtonElement | null) => {
        rootRef.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) (ref as React.MutableRefObject<HTMLButtonElement | null>).current = node;
    };

    useLayoutEffect(() => {
        const ind = indicatorRef.current;
        const root = rootRef.current;
        if (!ind) return;
        const isChecked = checked === true;
        if (!m.enabled) {
            gsap.set(ind, { autoAlpha: isChecked ? 1 : 0, scale: isChecked ? 1 : 0.4 });
            prevCheckedRef.current = checked;
            return;
        }
        if (isChecked) {
            gsap.fromTo(
                ind,
                { autoAlpha: 0, scale: 0.4 },
                {
                    autoAlpha: 1,
                    scale: 1,
                    duration: m.duration('fast'),
                    ease: m.ease.pop,
                },
            );
            // 勾上时给 root 一次 pop 反馈
            if (root && prevCheckedRef.current !== true) {
                m.pop(root, { peak: 1 + (m.preset.feel.popPeak - 1) * 0.6 });
            }
        } else {
            gsap.to(ind, {
                autoAlpha: 0,
                scale: 0.4,
                duration: m.duration('fast') * 0.6,
                ease: m.ease.exit,
            });
        }
        prevCheckedRef.current = checked;
    }, [checked, m]);

    const box = (
        <RadixCheckbox.Root
            ref={setRootRef}
            id={fieldId}
            disabled={disabled}
            checked={checked}
            onCheckedChange={(c) => onCheckedChange?.(c === true)}
            className={cn(
                'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-xs',
                'border border-border-strong bg-field transition-colors duration-200',
                'data-[state=checked]:border-brand data-[state=checked]:bg-brand',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                'disabled:cursor-not-allowed disabled:opacity-50',
            )}
            {...rest}
        >
            <RadixCheckbox.Indicator forceMount asChild>
                <span
                    ref={indicatorRef}
                    style={{ visibility: 'hidden', opacity: 0 }}
                    className="flex items-center justify-center text-white"
                >
                    <Check size={11} strokeWidth={3} />
                </span>
            </RadixCheckbox.Indicator>
        </RadixCheckbox.Root>
    );

    if (!label) return <div className={className}>{box}</div>;

    return (
        <label
            htmlFor={fieldId}
            className={cn(
                'inline-flex items-start gap-2 cursor-pointer',
                disabled && 'cursor-not-allowed opacity-60',
                className,
            )}
        >
            {box}
            <span className="flex flex-col gap-0.5 leading-tight">
                <span className="text-sm text-text">{label}</span>
                {hint && <span className="text-2xs text-text-tertiary">{hint}</span>}
            </span>
        </label>
    );
});
Checkbox.displayName = 'Checkbox';
