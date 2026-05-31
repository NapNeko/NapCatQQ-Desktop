// Select 原子件。Radix portal Select，A11y / 键盘 / 滚动 都有兜底。
//
// 简化对外 API：传 items 数组而不是手动嵌 SelectItem，调用方更短。
// 如需自定义 item 渲染（带图标、双行）后续可再 export 一个 SelectAdvanced。

import * as RadixSelect from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp } from 'lucide-react';
import { forwardRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export interface SelectItem<V extends string = string> {
    value: V;
    label: ReactNode;
    disabled?: boolean;
}

export interface SelectProps<V extends string = string> {
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    required?: boolean;
    placeholder?: string;
    items: ReadonlyArray<SelectItem<V>>;
    value: V | undefined;
    onValueChange: (value: V) => void;
    disabled?: boolean;
    className?: string;
    id?: string;
    name?: string;
}

function SelectInner<V extends string>(
    {
        label,
        hint,
        error,
        required,
        placeholder,
        items,
        value,
        onValueChange,
        disabled,
        className,
        id,
        name,
    }: SelectProps<V>,
    ref: React.Ref<HTMLButtonElement>,
) {
    const invalid = !!error;
    const fieldId = id ?? name;
    const describedById = fieldId ? `${fieldId}-desc` : undefined;
    const m = useMotion();

    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            {label && (
                <label
                    htmlFor={fieldId}
                    className="text-xs font-medium text-text-secondary"
                >
                    {label}
                    {required && <span className="ml-0.5 text-danger">*</span>}
                </label>
            )}
            <RadixSelect.Root
                value={value}
                onValueChange={(v) => onValueChange(v as V)}
                disabled={disabled}
                name={name}
            >
                <RadixSelect.Trigger
                    ref={ref}
                    id={fieldId}
                    aria-invalid={invalid || undefined}
                    aria-describedby={describedById}
                    className={cn(
                        'inline-flex w-full items-center justify-between gap-2 rounded-sm bg-field px-3 py-2',
                        'text-sm text-text border outline-none transition-colors',
                        'focus:ring-1 focus:ring-brand',
                        'data-[placeholder]:text-text-tertiary',
                        'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
                        invalid
                            ? 'border-danger focus:border-danger focus:ring-danger'
                            : 'border-border-subtle focus:border-brand',
                    )}
                >
                    <RadixSelect.Value placeholder={placeholder} />
                    <RadixSelect.Icon asChild>
                        <ChevronDown size={14} className="text-text-tertiary" />
                    </RadixSelect.Icon>
                </RadixSelect.Trigger>
                <RadixSelect.Portal>
                    <RadixSelect.Content
                        ref={(node) => {
                            if (!node) return;
                            // 进场动画:trigger 方向(data-side)缩放展开。Radix 在 mount
                            // 时立刻给 data-side。读不到时退化为顶部展开。
                            if (!m.enabled) return;
                            const side = node.getAttribute('data-side') ?? 'bottom';
                            const origin =
                                side === 'top' ? '50% 100%' : side === 'bottom' ? '50% 0%' : '50% 50%';
                            gsap.set(node, { transformOrigin: origin });
                            gsap.fromTo(
                                node,
                                { autoAlpha: 0, scale: 0.96, y: side === 'top' ? 4 : -4 },
                                {
                                    autoAlpha: 1,
                                    scale: 1,
                                    y: 0,
                                    duration: m.duration('fast'),
                                    ease: m.ease.enterMicro,
                                },
                            );
                        }}
                        position="popper"
                        sideOffset={4}
                        className={cn(
                            'z-50 overflow-hidden rounded-sm border border-border-subtle',
                            'bg-elevated shadow-popover',
                            'min-w-[var(--radix-select-trigger-width)]',
                        )}
                        style={{ visibility: 'hidden', opacity: 0 }}
                    >
                        <RadixSelect.ScrollUpButton className="flex h-6 cursor-default items-center justify-center bg-elevated text-text-tertiary">
                            <ChevronUp size={14} />
                        </RadixSelect.ScrollUpButton>
                        <RadixSelect.Viewport className="p-1">
                            {items.map((item) => (
                                <RadixSelect.Item
                                    key={item.value}
                                    value={item.value}
                                    disabled={item.disabled}
                                    className={cn(
                                        'relative flex cursor-pointer select-none items-center gap-2 rounded-xs',
                                        'px-2.5 py-1.5 pr-7 text-sm text-text',
                                        'data-[state=checked]:bg-brand-soft data-[state=checked]:text-brand',
                                        'data-[highlighted]:bg-inset data-[highlighted]:outline-none',
                                        'data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50',
                                    )}
                                >
                                    <RadixSelect.ItemText>{item.label}</RadixSelect.ItemText>
                                    <RadixSelect.ItemIndicator className="absolute right-2 inline-flex items-center">
                                        <Check size={12} strokeWidth={3} />
                                    </RadixSelect.ItemIndicator>
                                </RadixSelect.Item>
                            ))}
                        </RadixSelect.Viewport>
                        <RadixSelect.ScrollDownButton className="flex h-6 cursor-default items-center justify-center bg-elevated text-text-tertiary">
                            <ChevronDown size={14} />
                        </RadixSelect.ScrollDownButton>
                    </RadixSelect.Content>
                </RadixSelect.Portal>
            </RadixSelect.Root>
            {(hint || error) && (
                <p
                    id={describedById}
                    className={cn(
                        'text-2xs leading-snug',
                        invalid ? 'text-danger' : 'text-text-tertiary',
                    )}
                >
                    {error ?? hint}
                </p>
            )}
        </div>
    );
}

// 泛型 forwardRef 的 TS 怪规则：要先 cast 一下才能保留泛型签名。
export const Select = forwardRef(SelectInner) as <V extends string>(
    props: SelectProps<V> & { ref?: React.Ref<HTMLButtonElement> },
) => ReturnType<typeof SelectInner>;
