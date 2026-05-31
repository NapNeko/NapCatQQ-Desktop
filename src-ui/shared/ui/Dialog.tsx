// Dialog 原子件。基于 Radix Dialog,在外面包一层让 framer-motion 能正确做退场。
//
// 视觉决策:
//   - overlay 用半透明暖灰而不是纯黑(bg-canvas/55 + backdrop-blur),匹配整体暖色基调
//   - content 用 surface-elevated(暖米黄)+ shadow-popover,参考图风格
//   - 关闭按钮放右上,键盘 Esc 也可关(Radix 自动)
//
// 动画:进退场走 framer-motion + AnimatePresence,fade + scale 0.96→1。
// 之前这里用 `animate-in / fade-in-0 / zoom-in-95` 的 tailwindcss-animate 类,
// 但项目根本没装 tailwindcss-animate 插件——这些类是空的,Dialog 实际是瞬开瞬关。
// 现在统一接 framer,跟其它 motion 走同一档位/速度偏好。
//
// 实现要点:Radix 的 Root 自己控制 Portal mount/unmount,但 framer 需要先看到
// open=false 才能跑 exit。解法:在 wrapper Dialog 内开个 context 把 open 传下去,
// 由 DialogContent 用 AnimatePresence 自己判断挂卸,用 Radix 的 forceMount 保留
// Portal 不让 Radix 抢着卸载。Esc / 点击 overlay 等 a11y 行为仍由 Radix 兜底。

import * as RadixDialog from '@radix-ui/react-dialog';
import { AnimatePresence, motion } from 'framer-motion';
import { X as CloseIcon } from 'lucide-react';
import {
    createContext,
    forwardRef,
    useContext,
    useState,
    type ComponentPropsWithoutRef,
    type ElementRef,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';
import {
    dialogContentVariants,
    dialogOverlayVariants,
} from '../../core/design/motion';

const DialogOpenContext = createContext<boolean>(false);

interface DialogRootProps {
    open?: boolean;
    defaultOpen?: boolean;
    onOpenChange?: (open: boolean) => void;
    modal?: boolean;
    children?: ReactNode;
}

/// Dialog wrapper:对外接口和 RadixDialog.Root 完全一致,内部多走一层 context
/// 把 open 传给 DialogContent。受控/非受控两种用法都支持。
export function Dialog({
    open,
    defaultOpen,
    onOpenChange,
    modal,
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
            <RadixDialog.Root
                open={actualOpen}
                onOpenChange={handleChange}
                modal={modal}
            >
                {children}
            </RadixDialog.Root>
        </DialogOpenContext.Provider>
    );
}

export const DialogTrigger = RadixDialog.Trigger;
export const DialogClose = RadixDialog.Close;
export const DialogPortal = RadixDialog.Portal;

export const DialogOverlay = forwardRef<
    ElementRef<typeof RadixDialog.Overlay>,
    ComponentPropsWithoutRef<typeof RadixDialog.Overlay>
>(({ className, ...props }, ref) => {
    const m = useMotion();
    return (
        <RadixDialog.Overlay asChild forceMount {...props}>
            <motion.div
                ref={ref}
                variants={dialogOverlayVariants}
                initial="initial"
                animate="animate"
                exit="exit"
                transition={m.transition('base')}
                className={cn(
                    'fixed inset-0 z-50 bg-black/35 backdrop-blur-sm',
                    className,
                )}
            />
        </RadixDialog.Overlay>
    );
});
DialogOverlay.displayName = 'DialogOverlay';

interface DialogContentProps
    extends Omit<ComponentPropsWithoutRef<typeof RadixDialog.Content>, 'forceMount'> {
    hideClose?: boolean;
}

export const DialogContent = forwardRef<
    ElementRef<typeof RadixDialog.Content>,
    DialogContentProps
>(({ className, children, hideClose, ...props }, ref) => {
    const open = useContext(DialogOpenContext);
    const m = useMotion();
    return (
        <AnimatePresence>
            {open && (
                <RadixDialog.Portal forceMount>
                    <DialogOverlay />
                    <RadixDialog.Content asChild forceMount {...props}>
                        <motion.div
                            ref={ref}
                            variants={dialogContentVariants}
                            initial="initial"
                            animate="animate"
                            exit="exit"
                            transition={m.transition('base')}
                            className={cn(
                                'fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2',
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
                                    <CloseIcon size={16} />
                                </RadixDialog.Close>
                            )}
                        </motion.div>
                    </RadixDialog.Content>
                </RadixDialog.Portal>
            )}
        </AnimatePresence>
    );
});
DialogContent.displayName = 'DialogContent';

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
