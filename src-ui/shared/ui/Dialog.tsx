// Dialog 原子件。基于 Radix Dialog,在外面包 GSAP 动画层。
//
// 视觉决策:
//   - overlay 半透明黑 + backdrop-blur
//   - content surface-elevated + shadow-popover
//   - 关闭按钮右上,Esc 关
//
// 动画:进退场走 GSAP + GsapPresence。
// 实现要点:
//   - RadixDialog.Root open 是受控的 boolean,DialogContent 在外面挂一个
//     固定 Portal(forceMount)让 RadixDialog 不抢着卸载内部
//   - overlay/content 各自由 GsapPresence(visible=open) 控:open=true 就 mount + enter,
//     open=false 就跑 exit + 完成后真 unmount
//   - 整个 Portal 永远存在,不存在 Radix 强制立即卸载内部 children 的问题

import * as RadixDialog from '@radix-ui/react-dialog';
import { X as CloseIcon } from 'lucide-react';
import gsap from 'gsap';
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
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';

const DialogOpenContext = createContext<boolean>(false);

interface DialogRootProps {
    open?: boolean;
    defaultOpen?: boolean;
    onOpenChange?: (open: boolean) => void;
    modal?: boolean;
    children?: ReactNode;
}

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

const overlayEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0 },
        { autoAlpha: 1, duration: env.duration('base'), ease: env.preset.enterEase },
    );
const overlayExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        duration: env.duration('fast'),
        ease: env.preset.exitEase,
    });

const contentEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, scale: 0.96, y: -2 },
        {
            autoAlpha: 1,
            scale: 1,
            y: 0,
            duration: env.duration('base'),
            ease: env.preset.enterEase,
        },
    );
const contentExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        scale: 0.97,
        y: -2,
        duration: env.duration('fast'),
        ease: env.preset.exitEase,
    });

interface DialogContentProps
    extends Omit<ComponentPropsWithoutRef<typeof RadixDialog.Content>, 'forceMount'> {
    hideClose?: boolean;
}

/// DialogContent:外层固定 Portal,内部两块 GsapPresence 各自管 overlay/content。
/// 居中用 flex(不依赖 transform),让 GSAP 可以自由动 content 的 scale/y 不互相影响。
export const DialogContent = forwardRef<
    ElementRef<typeof RadixDialog.Content>,
    DialogContentProps
>(({ className, children, hideClose, ...props }, _ref) => {
    const open = useContext(DialogOpenContext);
    return (
        <RadixDialog.Portal forceMount>
            <GsapPresence visible={open} onEnter={overlayEnter} onExit={overlayExit}>
                <RadixDialog.Overlay asChild forceMount>
                    <OverlayBody />
                </RadixDialog.Overlay>
            </GsapPresence>
            {/* 居中容器:不动 transform,只用 flex 居中。pointer-events-none
                让点击穿透到 overlay,但内部 ContentBody 自己 pointer-events-auto。
                isolation:isolate 强制创建独立 stacking context,让 content 不被
                overlay 的 backdrop-filter 计算成"后方内容"误模糊。 */}
            <div
                style={{ isolation: 'isolate' }}
                className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-6"
            >
                <GsapPresence visible={open} onEnter={contentEnter} onExit={contentExit}>
                    <RadixDialog.Content asChild forceMount {...props}>
                        <ContentBody className={className} hideClose={hideClose}>
                            {children}
                        </ContentBody>
                    </RadixDialog.Content>
                </GsapPresence>
            </div>
        </RadixDialog.Portal>
    );
});
DialogContent.displayName = 'DialogContent';

const OverlayBody = forwardRef<HTMLDivElement, { className?: string }>(
    ({ className }, ref) => (
        <div
            ref={ref}
            style={{ visibility: 'hidden', opacity: 0 }}
            className={cn(
                // overlay z-40,内容 z-50,留出层级让 backdrop-blur 的"后方区"
                // 不会误把 content 也算进去(GSAP 给 content 加 transform 会创建
                // 新 stacking context,跟 overlay 同 z-50 时浏览器计算可能错乱)。
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
            // 不再 fixed + translate 居中(交给外层 flex 容器);自己只管视觉。
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
                <CloseIcon size={16} />
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

// DialogPortal 仅 re-export 给少数手动用法保持兼容。新代码用 DialogContent 即可,
// 内部已经包了 Portal。
export const DialogPortal = RadixDialog.Portal;
