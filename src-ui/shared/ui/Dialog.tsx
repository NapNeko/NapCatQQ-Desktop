// Dialog 原子件。基于 Radix Dialog,在外面包 GSAP 动画层。第二轮重写。
//
// 视觉决策:
//   - overlay 半透明黑 + backdrop-blur
//   - content surface-elevated + shadow-popover
//   - 关闭按钮右上,Esc 关
//
// 动画:进退场走 GSAP + GsapPresence。第二轮加入空间锚点:
//   - 通过 DialogAnchorContext 把"触发点"位置传进来(任意点击 trigger 之前
//     先通过 useDialogAnchor() 设置),enter 时从锚点缩小起源放大到屏中,exit
//     时反向收回锚点。standard/rich 档启用,elegant 档退化为单纯缩放。
//   - 没设置锚点时退化到原来的"中心 fade + scale"。
//
// 关闭策略:outside 默认只在点到遮罩层时关闭。原生 select 下拉在 portal 外，
// 以及 flex 居中层上的空白点击，不再误关弹窗。

import * as RadixDialog from '@radix-ui/react-dialog';
import { X as CloseIcon } from 'lucide-react';
import gsap from 'gsap';
import {
    createContext,
    forwardRef,
    useCallback,
    useContext,
    useState,
    type ComponentPropsWithoutRef,
    type ElementRef,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';
import { MotionIcon } from './motion/MotionIcon';

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
    /// false = 点遮罩也不关，只能点关闭按钮 / Esc（表单弹窗推荐）。
    dismissOnOutsideClick?: boolean;
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
            dismissOnOutsideClick = true,
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
                    className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-6"
                >
                    <GsapPresence visible={open} onEnter={contentEnter} onExit={contentExit}>
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
                            <ContentBody className={className} hideClose={hideClose}>
                                {children}
                            </ContentBody>
                        </RadixDialog.Content>
                    </GsapPresence>
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
                'fixed inset-0 z-40 bg-black/35 backdrop-blur-sm',
                className,
            )}
        />
    ),
);
OverlayBody.displayName = 'OverlayBody';

const ContentBody = forwardRef<
    HTMLDivElement,
    { className?: string; hideClose?: boolean; children?: ReactNode }
>(({ className, hideClose, children }, ref) => (
    <div
        ref={ref}
        style={{ visibility: 'hidden', opacity: 0 }}
        className={cn(
            'pointer-events-auto relative w-full max-w-md',
            'rounded-md bg-elevated p-6 shadow-popover',
            className,
        )}
    >
        {children}
        {!hideClose && (
            <RadixDialog.Close
                aria-label="关闭"
                className="absolute right-3 top-3 rounded-xs p-1 text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
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
));
ContentBody.displayName = 'ContentBody';

export const DialogHeader: React.FC<React.HTMLAttributes<HTMLDivElement>> = ({
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