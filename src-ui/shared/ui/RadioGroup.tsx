// RadioGroup 原子件。Radix 兜底 a11y / 箭头切换 / role="radiogroup"。
//
// 用 items 数组 API 简化使用：
//   <RadioGroup value={mode} onValueChange={setMode}
//     items={[
//       { value: 'cold', label: 'COLD：本程序启动新 QQ.exe' },
//       { value: 'hot', label: 'HOT：附加到已存在的 QQ.exe', hint: '保留人工登录会话' },
//     ]} />
//
// 默认垂直排，要水平排传 orientation="horizontal"。

import * as RadixRadio from '@radix-ui/react-radio-group';
import { forwardRef, type ReactNode } from 'react';
import { cn } from '../utils/cn';

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
                {items.map((item) => {
                    const itemId = `${name ?? 'rg'}-${item.value}`;
                    return (
                        <label
                            key={item.value}
                            htmlFor={itemId}
                            className={cn(
                                'inline-flex items-start gap-2 cursor-pointer',
                                (disabled || item.disabled) && 'cursor-not-allowed opacity-60',
                            )}
                        >
                            <RadixRadio.Item
                                id={itemId}
                                value={item.value}
                                disabled={item.disabled}
                                className={cn(
                                    'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full',
                                    'border border-border-strong bg-field transition-colors',
                                    'data-[state=checked]:border-brand',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1',
                                    'disabled:cursor-not-allowed disabled:opacity-50',
                                )}
                            >
                                <RadixRadio.Indicator className="h-2 w-2 rounded-full bg-brand" />
                            </RadixRadio.Item>
                            <span className="flex flex-col gap-0.5 leading-tight">
                                <span className="text-sm text-text">{item.label}</span>
                                {item.hint && (
                                    <span className="text-2xs text-text-tertiary">{item.hint}</span>
                                )}
                            </span>
                        </label>
                    );
                })}
            </RadixRadio.Root>
        </div>
    );
}

export const RadioGroup = forwardRef(RadioGroupInner) as <V extends string>(
    props: RadioGroupProps<V> & { ref?: React.Ref<HTMLDivElement> },
) => ReturnType<typeof RadioGroupInner>;
