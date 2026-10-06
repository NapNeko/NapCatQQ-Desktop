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
import { MotionIcon } from './motion/MotionIcon';

// 进场动画挂在 Content 的 ref 回调上，但 ref 回调不是「挂载一次」：Radix 内部把我们的 ref
// 和一个内联箭头函数组合在一起，SelectContentImpl 每重渲一次（打开过程中要连着好几次）就换一个
// 组合 ref，同一个节点被反复传进来，父级重渲也一样。以前每传一次就从透明重播一遍，打开时面板
// 一闪一闪。每次打开 Radix 都新建内容节点，按节点记住已进场的就够了。
const enteredContent = new WeakSet<Element>();

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
    // Radix Select：空字符串会抛错，统一当未选。
    const radixValue = value === '' || value === undefined ? undefined : value;

    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            {label && (
                <label htmlFor={fieldId} className="text-xs font-medium text-text-secondary">
                    {label}
                    {required && <span className="ml-0.5 text-danger">*</span>}
                </label>
            )}
            <RadixSelect.Root
                value={radixValue}
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
                        // inset ring：避免 offset 被 overflow 父级裁切
                        'focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-inset',
                        'data-[state=open]:ring-2 data-[state=open]:ring-brand data-[state=open]:ring-inset',
                        'focus:outline-none',
                        'data-[placeholder]:text-text-tertiary',
                        'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
                        invalid
                            ? 'border-danger focus-visible:border-danger focus-visible:ring-danger data-[state=open]:border-danger data-[state=open]:ring-danger'
                            : 'border-border-subtle focus-visible:border-brand data-[state=open]:border-brand',
                    )}
                >
                    <RadixSelect.Value placeholder={placeholder} />
                    <RadixSelect.Icon asChild>
                        <MotionIcon
                            icon={ChevronDown}
                            motion="none"
                            playEnter={false}
                            size={14}
                            className="text-text-tertiary"
                        />
                    </RadixSelect.Icon>
                </RadixSelect.Trigger>
                <RadixSelect.Portal>
                    <RadixSelect.Content
                        ref={(node) => {
                            if (!node || enteredContent.has(node)) return;
                            enteredContent.add(node);
                            // Content 可能以 hidden 起手；无论动效开/关，最终都必须可见。
                            // 旧 bug：!m.enabled 时直接 return，下拉挂在 DOM 里但永远看不见。
                            gsap.killTweensOf(node);
                            if (!m.enabled) {
                                gsap.set(node, { autoAlpha: 1, scale: 1, y: 0 });
                                return;
                            }
                            // 进场动画:trigger 方向(data-side)缩放展开。Radix 在 mount
                            // 时立刻给 data-side。读不到时退化为顶部展开。
                            const side = node.getAttribute('data-side') ?? 'bottom';
                            const origin =
                                side === 'top'
                                    ? '50% 100%'
                                    : side === 'bottom'
                                      ? '50% 0%'
                                      : '50% 50%';
                            const dur = m.duration('fast');
                            gsap.set(node, { transformOrigin: origin });
                            // duration 异常为 0 时 fromTo 可能不跑完，直接落到可见终态。
                            if (dur <= 0) {
                                gsap.set(node, { autoAlpha: 1, scale: 1, y: 0 });
                                return;
                            }
                            gsap.fromTo(
                                node,
                                { autoAlpha: 0, scale: 0.96, y: side === 'top' ? 4 : -4 },
                                {
                                    autoAlpha: 1,
                                    scale: 1,
                                    y: 0,
                                    duration: dur,
                                    ease: m.ease.enterMicro,
                                    // 被 kill / 中断时也落到可见，避免偶发卡在 hidden。
                                    onInterrupt: () => {
                                        gsap.set(node, { autoAlpha: 1, scale: 1, y: 0 });
                                    },
                                },
                            );
                        }}
                        position="popper"
                        sideOffset={4}
                        className={cn(
                            'z-50 overflow-hidden rounded-sm border border-border-subtle',
                            'bg-elevated shadow-popover',
                            'min-w-[var(--radix-select-trigger-width)]',
                            // 限高交给 Radix 算出的可用空间（它知道触发器在视口哪一侧、
                            // 上下还剩多少）。不设的话：选项一多面板就撑到视口外，
                            // 配合下面的 overflow-hidden 表现为「后半截选不到也滚不动」。
                            // 三个孩子（上按钮 / Viewport / 下按钮）纵向排列，让 Viewport 拿到剩余高度。
                            // 不加 flex-col 的话 Viewport 高度不受约束，overflow-y-auto 也就永远不触发。
                            'flex max-h-[var(--radix-select-content-available-height)] flex-col',
                        )}
                        // 关动效 / reduced-motion：不写 hidden，不依赖 GSAP。
                        // 开动效：先 hidden，由上面 ref 回调 fromTo 露出。
                        style={m.enabled ? { visibility: 'hidden', opacity: 0 } : undefined}
                    >
                        <RadixSelect.ScrollUpButton className="flex h-6 shrink-0 cursor-default items-center justify-center bg-elevated text-text-tertiary">
                            <ChevronUp size={14} />
                        </RadixSelect.ScrollUpButton>
                        <RadixSelect.Viewport
                            className={cn(
                                // min-h-0 是 flex 子项能收缩、从而让 overflow 生效的前提
                                'min-h-0 flex-1 p-1',
                                // 超高时滚动条出现在这里。Radix 的滚动按钮靠 Viewport 的
                                // scrollTop 工作，所以滚动必须在 Viewport 上而不是 Content。
                                'overflow-y-auto overscroll-contain',
                            )}
                        >
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
                        <RadixSelect.ScrollDownButton className="flex h-6 shrink-0 cursor-default items-center justify-center bg-elevated text-text-tertiary">
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
