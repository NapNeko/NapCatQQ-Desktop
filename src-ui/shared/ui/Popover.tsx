// Popover 原子件。基于 Radix Popover + GSAP 进退场动画。
//
// 设计决策:
//   - 进退场走 useGSAP(open 信号),与 Dialog 同源模式
//   - 进场:容器从 trigger 侧缩放展开(scale 0.92 + autoAlpha + y 位移),
//     standard/rich 档用 back.out 曲线带轻弹;elegant 档退化 power2.out
//     容器就绪后直接子元素用 stagger 分批滑入(standard/rich 档)
//   - 退场:scale 0.95 + autoAlpha → 0,比进场更快更收敛
//   - collisionPadding 默认 12px,避免贴窗体边缘被裁切
//   - 视觉:bg-elevated + shadow-popover + border-border-subtle,
//     与 Dialog/Tooltip 保持同一套语义 token
//   - Arrow 颜色跟随 elevated + border-subtle,适配所有主题

import * as RadixPopover from '@radix-ui/react-popover';
import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import {
    createContext,
    forwardRef,
    useContext,
    useRef,
    useState,
    type ComponentPropsWithoutRef,
    type ElementRef,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

gsap.registerPlugin(useGSAP);

// ---------- Context ----------

const PopoverOpenContext = createContext<boolean>(false);

// ---------- Root ----------

interface PopoverProps {
    open?: boolean;
    defaultOpen?: boolean;
    onOpenChange?: (open: boolean) => void;
    modal?: boolean;
    children?: ReactNode;
}

export function Popover({
    open,
    defaultOpen,
    onOpenChange,
    modal = false,
    children,
}: PopoverProps) {
    const isControlled = open !== undefined;
    const [internal, setInternal] = useState<boolean>(defaultOpen ?? false);
    const actualOpen = isControlled ? open! : internal;
    const handleChange = (next: boolean) => {
        if (!isControlled) setInternal(next);
        onOpenChange?.(next);
    };
    return (
        <PopoverOpenContext.Provider value={actualOpen}>
            <RadixPopover.Root
                open={actualOpen}
                onOpenChange={handleChange}
                modal={modal}
            >
                {children}
            </RadixPopover.Root>
        </PopoverOpenContext.Provider>
    );
}

export const PopoverTrigger = RadixPopover.Trigger;
export const PopoverClose = RadixPopover.Close;
export const PopoverAnchor = RadixPopover.Anchor;

// ---------- Content ----------

function originForSide(side: string): string {
    switch (side) {
        case 'top':
            return '50% 100%';
        case 'bottom':
            return '50% 0%';
        case 'left':
            return '100% 50%';
        case 'right':
            return '0% 50%';
        default:
            return '50% 0%';
    }
}

/** y 偏移方向:bottom 侧向下弹入,top 侧向上弹入,left/right 水平弹入。 */
function enterOffset(side: string): { x: number; y: number } {
    switch (side) {
        case 'top':
            return { x: 0, y: 6 };
        case 'bottom':
            return { x: 0, y: -6 };
        case 'left':
            return { x: 6, y: 0 };
        case 'right':
            return { x: -6, y: 0 };
        default:
            return { x: 0, y: -6 };
    }
}

interface PopoverContentProps
    extends Omit<ComponentPropsWithoutRef<typeof RadixPopover.Content>, 'forceMount'> {
    children?: ReactNode;
}

export const PopoverContent = forwardRef<
    ElementRef<typeof RadixPopover.Content>,
    PopoverContentProps
>(
    (
        {
            className,
            children,
            side = 'bottom',
            sideOffset = 6,
            align = 'center',
            ...contentProps
        },
        _ref,
    ) => {
        const open = useContext(PopoverOpenContext);
        const m = useMotion();
        const elRef = useRef<HTMLDivElement | null>(null);

        // 首次打开后才挂载 Portal，彻底避免冷启动 forceMount DOM 遮挡下层点击
        const [hasBeenOpened, setHasBeenOpened] = useState(false);
        if (open && !hasBeenOpened) setHasBeenOpened(true);

        useGSAP(
            () => {
                const el = elRef.current;
                if (!el) return;

                // 进场前确保 display 可见（退场动画完成时会设 display:none 解除遮挡）
                gsap.set(el, { display: '' });

                if (open) {
                    // ENTER — 容器 scale+fade 从 trigger 侧弹入
                    gsap.killTweensOf(el);
                    gsap.killTweensOf(el.children);
                    const origin = originForSide(side as string);
                    gsap.set(el, { transformOrigin: origin });

                    if (!m.enabled) {
                        gsap.set(el, { autoAlpha: 1, scale: 1, x: 0, y: 0 });
                        gsap.set(el.children, { autoAlpha: 1, y: 0 });
                        return;
                    }

                    const off = enterOffset(side as string);
                    const dur = m.duration('base');
                    const staggerVal = m.stagger();

                    // 容器 timeline
                    const tl = gsap.timeline();
                    tl.fromTo(
                        el,
                        { autoAlpha: 0, scale: 0.92, x: off.x, y: off.y },
                        {
                            autoAlpha: 1,
                            scale: 1,
                            x: 0,
                            y: 0,
                            duration: dur,
                            ease: m.ease.enter,
                            onComplete: () => {
                                gsap.set(el, { clearProps: 'transform' });
                            },
                        },
                    );

                    // standard/rich 档:子元素 stagger 分批滑入
                    if (staggerVal > 0 && el.children.length > 1) {
                        gsap.set(el.children, { autoAlpha: 0, y: 4 });
                        tl.fromTo(
                            el.children,
                            { autoAlpha: 0, y: 4 },
                            {
                                autoAlpha: 1,
                                y: 0,
                                duration: dur * 0.8,
                                ease: m.ease.enterMicro,
                                stagger: staggerVal,
                            },
                            '-=60%',
                        );
                    }
                } else {
                    // EXIT — 收敛更快,scale 0.95 + fade，完成后设 display:none 解除 pointer 遮挡
                    gsap.killTweensOf(el);
                    gsap.killTweensOf(el.children);
                    if (!m.enabled) {
                        gsap.set(el, { autoAlpha: 0, display: 'none' });
                        return;
                    }
                    gsap.to(el, {
                        autoAlpha: 0,
                        scale: 0.95,
                        duration: m.duration('fast') * 0.6,
                        ease: m.ease.exit,
                        onComplete: () => {
                            gsap.set(el, { display: 'none' });
                        },
                    });
                }
            },
            { dependencies: [open, m.enabled, side, hasBeenOpened] },
        );

        // 冷启动：Portal 始终渲染（让 Radix 正常工作），但 Content 延迟到首次打开后才挂载
        return (
            <RadixPopover.Portal forceMount>
                {(open || hasBeenOpened) && (
                    <RadixPopover.Content
                        ref={(node) => {
                            elRef.current = node;
                            if (typeof _ref === 'function') _ref(node);
                            else if (_ref)
                                (_ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
                        }}
                        side={side}
                        sideOffset={sideOffset}
                        align={align}
                        collisionPadding={12}
                        forceMount
                        style={{
                            visibility: 'hidden',
                            opacity: 0,
                            // 关闭时禁用指针（安全兜底），打开时由 GSAP autoAlpha 管理
                            ...(!open && { pointerEvents: 'none' as const }),
                        }}
                        className={cn(
                            'z-50 rounded-lg border border-border-subtle bg-elevated p-3 shadow-popover',
                            className,
                        )}
                        {...contentProps}
                    >
                        {children}
                        <RadixPopover.Arrow
                            className="fill-elevated drop-shadow-[0_0.5px_0_var(--color-border-subtle)]"
                            width={10}
                            height={5}
                        />
                    </RadixPopover.Content>
                )}
            </RadixPopover.Portal>
        );
    },
);
PopoverContent.displayName = 'PopoverContent';
