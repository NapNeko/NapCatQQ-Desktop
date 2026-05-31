// RadioGroup 原子件。第二轮:每个 RadioItem 的 indicator 接入 GSAP pop。
//
// 跟 Checkbox 类似,Radix RadioIndicator 默认 conditional 渲染。我们用 forceMount
// + 自控 visibility,checked 切换时让 GSAP pop。

import * as RadixRadio from '@radix-ui/react-radio-group';
import { forwardRef, useLayoutEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export interface RadioItem<V extends string = string> {
    value: V;
    label: ReactNode;
    /** label 下方的灰色辅助说明。 */
    hint?: ReactNode;
    disabled?: boolean;
}

export interface RadioGroupProps<V extends string = string> {
    label?: ReactNode;
    items: ReadonlyArray<RadioItem<V>>;
    value: V | undefined;
    onValueChange: (value: V) => void;
    orientation?: 'vertical' | 'horizontal';
    disabled?: boolean;
    className?: string;
    name?: string;
}

function RadioGroupInner<V extends string>(
    {
        label,
        items,
        value,
        onValueChange,
        orientation = 'vertical',
        disabled,
        className,
        name,
    }: RadioGroupProps<V>,
    ref: React.Ref<HTMLDivElement>,
) {
    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            {label && (
                <span className="text-xs font-medium text-text-secondary">{label}</span>
            )}
            <RadixRadio.Root
                ref={ref}
                value={value}
                onValueChange={(v) => onValueChange(v as V)}
                disabled={disabled}
                name={name}
                orientation={orientation}
                className={cn(
                    'flex',
                    orientation === 'vertical' ? 'flex-col gap-2' : 'flex-row flex-wrap gap-4',
                )}
            >
                {items.map((item) => (
                    <RadioItemView
                        key={item.value}
                        item={item}
                        name={name ?? 'rg'}
                        groupDisabled={disabled}
                        isSelected={value === item.value}
                    />
                ))}
            </RadixRadio.Root>
        </div>
    );
}

interface RadioItemViewProps<V extends string> {
    item: RadioItem<V>;
    name: string;
    groupDisabled?: boolean;
    isSelected: boolean;
}

function RadioItemView<V extends string>({
    item,
    name,
    groupDisabled,
    isSelected,
}: RadioItemViewProps<V>) {
    const m = useMotion();
    const itemId = `${name}-${item.value}`;
    const dotRef = useRef<HTMLSpanElement | null>(null);
    const prevSelectedRef = useRef<boolean>(isSelected);

    useLayoutEffect(() => {
        const dot = dotRef.current;
        if (!dot) return;
        if (!m.enabled) {
            gsap.set(dot, { autoAlpha: isSelected ? 1 : 0, scale: isSelected ? 1 : 0.3 });
            prevSelectedRef.current = isSelected;
            return;
        }
        if (isSelected) {
            gsap.fromTo(
                dot,
                { autoAlpha: 0, scale: 0.3 },
                {
                    autoAlpha: 1,
                    scale: 1,
                    duration: m.duration('fast'),
                    ease: m.ease.pop,
                },
            );
        } else if (prevSelectedRef.current) {
            gsap.to(dot, {
                autoAlpha: 0,
                scale: 0.3,
                duration: m.duration('fast') * 0.6,
                ease: m.ease.exit,
            });
        } else {
            gsap.set(dot, { autoAlpha: 0, scale: 0.3 });
        }
        prevSelectedRef.current = isSelected;
    }, [isSelected, m]);

    return (
        <label
            htmlFor={itemId}
            className={cn(
                'inline-flex items-start gap-2 cursor-pointer',
                (groupDisabled || item.disabled) && 'cursor-not-allowed opacity-60',
            )}
        >
            <RadixRadio.Item
                id={itemId}
                value={item.value}
                disabled={item.disabled}
                className={cn(
                    'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full',
                    'border border-border-strong bg-field transition-colors duration-200',
                    'data-[state=checked]:border-brand',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                    'disabled:cursor-not-allowed disabled:opacity-50',
                )}
            >
                <RadixRadio.Indicator forceMount asChild>
                    <span
                        ref={dotRef}
                        style={{ visibility: 'hidden', opacity: 0 }}
                        className="h-2 w-2 rounded-full bg-brand"
                    />
                </RadixRadio.Indicator>
            </RadixRadio.Item>
            <span className="flex flex-col gap-0.5 leading-tight">
                <span className="text-sm text-text">{item.label}</span>
                {item.hint && (
                    <span className="text-2xs text-text-tertiary">{item.hint}</span>
                )}
            </span>
        </label>
    );
}

export const RadioGroup = forwardRef(RadioGroupInner) as <V extends string>(
    props: RadioGroupProps<V> & { ref?: React.Ref<HTMLDivElement> },
) => ReturnType<typeof RadioGroupInner>;
