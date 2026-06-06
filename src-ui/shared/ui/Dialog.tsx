// Dialog 原子件。基于 Radix Dialog,在外面包 GSAP 动画层。第二轮重写。
//
// 视觉决策:
//   - overlay 半透明黑(无 backdrop-blur,减轻与弹窗内按钮 hover 叠层时的重影)
//   - content surface-elevated + shadow-popover
//   - 关闭按钮右上,Esc 关
//
// 动画:进退场走 GSAP + GsapPresence。第二轮加入空间锚点:
//   - 通过 DialogAnchorContext 把"触发点"位置传进来(任意点击 trigger 之前
//     先通过 useDialogAnchor() 设置),enter 时从锚点缩小起源放大到屏中,exit
//     时反向收回锚点。standard/rich 档启用,elegant 档退化为单纯缩放。
//   - 没设置锚点时退化到原来的"中心 fade + scale"。
//
//   - 打开期间高度:内层 clip 单独做 GSAP height(与进场 scale 不同节点,减轻重影)。
//   - 多步骤内容可用 DialogStepTransition 做步骤淡入。

import * as RadixDialog from '@radix-ui/react-dialog';
import { X as CloseIcon } from 'lucide-react';
import gsap from 'gsap';
import {
    createContext,
    forwardRef,
    useCallback,
    useContext,
    useEffect,
    useRef,
    useState,
    type ComponentPropsWithoutRef,
    type ElementRef,
    type HTMLAttributes,
    type MutableRefObject,
    type ReactNode,
    type RefObject,
} from 'react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';
import { MotionIcon } from './motion/MotionIcon';
import { DIALOG_SIZE_CLASS, type DialogSize } from './dialogSizes';

const DialogOpenContext = createContext<boolean>(false);

/// 锚点坐标(viewport 视口)。用户点击触发按钮时由调用方写入,Dialog 进退场
/// 用它做缩放原点 + 起始位移。
interface DialogAnchor {
    x: number;
    y: number;
}
const DialogAnchorContext = createContext<DialogAnchor | null>(null);

const OVERLAY_ATTR = 'data-dialog-overlay';

/// 给业务用:在按钮 onClick 里调用,捕获鼠标点位置后再 setOpen(true)。
/// 返回的对象 { anchor, setAnchor, captureFromEvent } 可挂到任意按钮事件上。
export function useDialogAnchor() {
    const [anchor, setAnchor] = useState<DialogAnchor | null>(null);
    const captureFromEvent = useCallback((e: React.MouseEvent | MouseEvent) => {
        setAnchor({ x: e.clientX, y: e.clientY });
    }, []);
    return { anchor, setAnchor, captureFromEvent };
}

interface DialogRootProps {
    open?: boolean;
    defaultOpen?: boolean;
    onOpenChange?: (open: boolean) => void;
    modal?: boolean;
    /// 锚点(viewport 视口);传入后 Dialog 进退场走"从锚点缩放"。
    /// 不传则退化到屏幕中心 scale。
    anchor?: DialogAnchor | null;
    children?: ReactNode;
}

export function Dialog({
    open,
    defaultOpen,
    onOpenChange,
    modal,
    anchor,
    children,
}: DialogRootProps) {
    const isControlled = open !== undefined;
    const [internal, setInternal] = useState<boolean>(defaultOpen ?? false);
    const actualOpen = isControlled ? open! : internal;
    const handleChange = (next: boolean) => {
        if (!isControlled) setInternal(next);
        onOpenChange?.(next);
    };
    return (
        <DialogOpenContext.Provider value={actualOpen}>
            <DialogAnchorContext.Provider value={anchor ?? null}>
                <RadixDialog.Root
                    open={actualOpen}
                    onOpenChange={handleChange}
                    modal={modal}
                >
                    {children}
                </RadixDialog.Root>
            </DialogAnchorContext.Provider>
        </DialogOpenContext.Provider>
    );
}

export const DialogTrigger = RadixDialog.Trigger;
export const DialogClose = RadixDialog.Close;

const overlayEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0 },
        { autoAlpha: 1, duration: env.duration('base'), ease: env.ease.enter },
    );
const overlayExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });

/// 计算锚点相对 content 中心的偏移和缩放原点。anchor 没传时回退到"中心 0,0"。
function makeContentEnter(anchor: DialogAnchor | null): EnterFn {
    return (el, env) => {
        const f = env.preset.feel;
        const rect = el.getBoundingClientRect();
        let originX = '50%';
        let originY = '50%';
        let dx = 0;
        let dy = 0;
        if (anchor && f.cardLift > 0) {
            const cx = rect.left + rect.width / 2;
            const cy = rect.top + rect.height / 2;
            originX = `${((anchor.x - rect.left) / rect.width) * 100}%`;
            originY = `${((anchor.y - rect.top) / rect.height) * 100}%`;
            dx = Math.max(-32, Math.min(32, (anchor.x - cx) * 0.18));
            dy = Math.max(-32, Math.min(32, (anchor.y - cy) * 0.18));
        }
        gsap.set(el, { transformOrigin: `${originX} ${originY}` });
        return gsap.fromTo(
            el,
            { autoAlpha: 0, scale: 0.92, x: dx, y: dy - 2 },
            {
                autoAlpha: 1,
                scale: 1,
                x: 0,
                y: 0,
                duration: env.duration('base'),
                ease: env.ease.enter,
                onComplete: () => {
                    gsap.set(el, { clearProps: 'transform' });
                },
            },
        );
    };
}

const contentExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        scale: 0.97,
        y: -2,
        duration: env.duration('fast'),
        ease: env.ease.exit,
        onComplete: () => {
            gsap.set(el, { clearProps: 'transform' });
        },
    });

function applyOutsideDismissGuard(
    e: { preventDefault: () => void; defaultPrevented: boolean; target: EventTarget | null },
    dismissOnOutsideClick: boolean,
) {
    if (e.defaultPrevented) return;
    if (!dismissOnOutsideClick) {
        e.preventDefault();
        return;
    }
    const target = e.target;
    if (!(target instanceof HTMLElement)) {
        e.preventDefault();
        return;
    }
    if (!target.closest(`[${OVERLAY_ATTR}]`)) {
        e.preventDefault();
    }
}

interface DialogContentProps
    extends Omit<ComponentPropsWithoutRef<typeof RadixDialog.Content>, 'forceMount'> {
    hideClose?: boolean;
    /// 预设宽度；高度在打开期间随内容变化由 GSAP 过渡。
    size?: DialogSize;
    /// false = 点遮罩也不关，只能点关闭按钮 / Esc（表单弹窗推荐）。
    dismissOnOutsideClick?: boolean;
    /// 内容区退场动画结束后触发。用于在 open=false 后延迟卸载 children，避免收起动画中途闪空。
    onExited?: () => void;
}

export const DialogContent = forwardRef<
    ElementRef<typeof RadixDialog.Content>,
    DialogContentProps
>(
    (
        {
            className,
            children,
            hideClose,
            size = 'md',
            dismissOnOutsideClick = true,
            onExited,
            onPointerDownOutside: onPointerDownOutsideProp,
            onFocusOutside: onFocusOutsideProp,
            onInteractOutside: onInteractOutsideProp,
            ...contentProps
        },
        _ref,
    ) => {
        const open = useContext(DialogOpenContext);
        const anchor = useContext(DialogAnchorContext);
        const contentEnter = makeContentEnter(anchor);

        return (
            <RadixDialog.Portal forceMount>
                <GsapPresence visible={open} onEnter={overlayEnter} onExit={overlayExit}>
                    <RadixDialog.Overlay asChild forceMount>
                        <OverlayBody />
                    </RadixDialog.Overlay>
                </GsapPresence>
                <div
                    style={{ isolation: 'isolate' }}
                    className="pointer-events-none fixed inset-0 z-50 overflow-y-auto"
                >
                    <div className="flex min-h-full items-center justify-center p-6">
                    <GsapPresence
                        visible={open}
                        onEnter={contentEnter}
                        onExit={contentExit}
                        onExited={onExited}
                    >
                        <RadixDialog.Content
                            asChild
                            forceMount
                            {...contentProps}
                            onPointerDownOutside={(e) => {
                                onPointerDownOutsideProp?.(e);
                                applyOutsideDismissGuard(e, dismissOnOutsideClick);
                            }}
                            onFocusOutside={(e) => {
                                onFocusOutsideProp?.(e);
                                applyOutsideDismissGuard(e, dismissOnOutsideClick);
                            }}
                            onInteractOutside={(e) => {
                                onInteractOutsideProp?.(e);
                                applyOutsideDismissGuard(e, dismissOnOutsideClick);
                            }}
                        >
                            <ContentBody
                                className={cn(DIALOG_SIZE_CLASS[size], className)}
                                size={size}
                                hideClose={hideClose}
                            >
                                {children}
                            </ContentBody>
                        </RadixDialog.Content>
                    </GsapPresence>
                    </div>
                </div>
            </RadixDialog.Portal>
        );
    },
);
DialogContent.displayName = 'DialogContent';

const OverlayBody = forwardRef<HTMLDivElement, { className?: string }>(
    ({ className }, ref) => (
        <div
            ref={ref}
            {...{ [OVERLAY_ATTR]: '' }}
            style={{ visibility: 'hidden', opacity: 0 }}
            className={cn(
                'fixed inset-0 z-40 bg-black/40',
                className,
            )}
        />
    ),
);
OverlayBody.displayName = 'OverlayBody';

function contentHeightCap(size: DialogSize): number {
    if (size === 'sheet') return Math.floor(window.innerHeight * 0.85);
    return Math.floor(window.innerHeight - 48);
}

function useDialogContentHeight(
    clipRef: RefObject<HTMLDivElement | null>,
    innerRef: RefObject<HTMLDivElement | null>,
    open: boolean,
    size: DialogSize,
) {
    const m = useMotion();
    const tweenRef = useRef<gsap.core.Tween | null>(null);
    const primedRef = useRef(false);
    const enterHoldRef = useRef(false);
    const prevOpenRef = useRef(open);

    useEffect(() => {
        if (open && !prevOpenRef.current) {
            primedRef.current = false;
            enterHoldRef.current = true;
            const t = window.setTimeout(() => {
                enterHoldRef.current = false;
            }, 420);
            prevOpenRef.current = open;
            return () => window.clearTimeout(t);
        }
        prevOpenRef.current = open;
        return undefined;
    }, [open]);

    useEffect(() => {
        const clip = clipRef.current;
        const inner = innerRef.current;
        if (!clip || !inner) return;

        const apply = () => {
            const cap = contentHeightCap(size);
            const raw = inner.scrollHeight;
            const target = Math.min(raw, cap);
            const scrollable = raw > cap + 1;

            inner.style.overflowY = scrollable ? 'auto' : '';
            inner.style.maxHeight = scrollable ? `${cap}px` : '';

            if (!open || !m.enabled) {
                tweenRef.current?.kill();
                clip.style.height = scrollable ? `${cap}px` : 'auto';
                primedRef.current = false;
                return;
            }

            if (!primedRef.current || enterHoldRef.current) {
                tweenRef.current?.kill();
                clip.style.height = `${target}px`;
                primedRef.current = true;
                return;
            }

            const from = clip.offsetHeight;
            if (Math.abs(target - from) < 2) return;

            tweenRef.current?.kill();
            tweenRef.current = gsap.fromTo(
                clip,
                { height: from },
                {
                    height: target,
                    duration: m.duration('base'),
                    ease: m.ease.damped,
                    overwrite: 'auto',
                },
            );
        };

        const ro = new ResizeObserver(() => requestAnimationFrame(apply));
        ro.observe(inner);
        apply();

        return () => {
            ro.disconnect();
            tweenRef.current?.kill();
            tweenRef.current = null;
        };
    }, [open, size, m.enabled, m.level, m.speed]);
}

const ContentBody = forwardRef<
    HTMLDivElement,
    { className?: string; size?: DialogSize; hideClose?: boolean; children?: ReactNode }
>(({ className, size = 'md', hideClose, children }, ref) => {
    const open = useContext(DialogOpenContext);
    const clipRef = useRef<HTMLDivElement | null>(null);
    const innerRef = useRef<HTMLDivElement | null>(null);
    useDialogContentHeight(clipRef, innerRef, open, size);

    const setOuterRef = (node: HTMLDivElement | null) => {
        if (typeof ref === 'function') ref(node);
        else if (ref) (ref as MutableRefObject<HTMLDivElement | null>).current = node;
    };

    return (
        <div
            ref={setOuterRef}
            style={{ visibility: 'hidden', opacity: 0 }}
            className={cn(
                'pointer-events-auto relative w-full',
                'rounded-md bg-elevated p-6 shadow-popover',
                size === 'sheet' && 'flex max-h-[85dvh] flex-col',
                'transition-[max-width] duration-300 ease-out',
                className,
            )}
        >
            <div
                ref={clipRef}
                className={cn('overflow-hidden', size === 'sheet' && 'min-h-0 flex-1')}
            >
                <div ref={innerRef} className={cn(size === 'sheet' && 'min-h-0')}>
                    {children}
                </div>
            </div>
            {!hideClose && (
                <RadixDialog.Close
                    aria-label="关闭"
                    className="absolute right-3 top-3 z-10 rounded-xs p-1 text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                >
                    <MotionIcon
                        icon={CloseIcon}
                        motion="none"
                        hoverAccent
                        playEnter={false}
                        size={16}
                    />
                </RadixDialog.Close>
            )}
        </div>
    );
});
ContentBody.displayName = 'ContentBody';

export const DialogHeader: React.FC<HTMLAttributes<HTMLDivElement>> = ({
    className,
    ...props
}) => <div className={cn('mb-3 flex flex-col gap-1', className)} {...props} />;

export const DialogTitle = forwardRef<
    ElementRef<typeof RadixDialog.Title>,
    ComponentPropsWithoutRef<typeof RadixDialog.Title>
>(({ className, ...props }, ref) => (
    <RadixDialog.Title
        ref={ref}
        className={cn('font-display text-md font-semibold text-text', className)}
        {...props}
    />
));
DialogTitle.displayName = 'DialogTitle';

export const DialogDescription = forwardRef<
    ElementRef<typeof RadixDialog.Description>,
    ComponentPropsWithoutRef<typeof RadixDialog.Description>
>(({ className, ...props }, ref) => (
    <RadixDialog.Description
        ref={ref}
        className={cn('text-sm text-text-secondary', className)}
        {...props}
    />
));
DialogDescription.displayName = 'DialogDescription';

export const DialogFooter: React.FC<React.HTMLAttributes<HTMLDivElement>> = ({
    className,
    ...props
}) => (
    <div
        className={cn('mt-5 flex items-center justify-end gap-2', className)}
        {...props}
    />
);

export const DialogPortal = RadixDialog.Portal;