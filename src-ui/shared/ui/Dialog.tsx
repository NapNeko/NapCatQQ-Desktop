// Dialog 原子件。基于 Radix，承担"批量删除确认 / 删除单 Bot 确认 / 错误详情"等场景。
//
// 视觉决策：
//   - overlay 用半透明暖灰而不是纯黑（bg-canvas/55 + backdrop-blur），匹配整体暖色基调
//   - content 用 surface-elevated（暖米黄）+ shadow-popover，参考图风格
//   - 关闭按钮放右上，键盘 Esc 也可关（Radix 自动）

import * as RadixDialog from '@radix-ui/react-dialog';
import { X as CloseIcon } from 'lucide-react';
import { forwardRef } from 'react';
import { cn } from '../utils/cn';

export const Dialog = RadixDialog.Root;
export const DialogTrigger = RadixDialog.Trigger;
export const DialogClose = RadixDialog.Close;
export const DialogPortal = RadixDialog.Portal;

export const DialogOverlay = forwardRef<
    React.ElementRef<typeof RadixDialog.Overlay>,
    React.ComponentPropsWithoutRef<typeof RadixDialog.Overlay>
>(({ className, ...props }, ref) => (
    <RadixDialog.Overlay
        ref={ref}
        className={cn(
            'fixed inset-0 z-50 bg-black/35 backdrop-blur-sm',
            'data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0',
            className,
        )}
        {...props}
    />
));
DialogOverlay.displayName = 'DialogOverlay';

export const DialogContent = forwardRef<
    React.ElementRef<typeof RadixDialog.Content>,
    React.ComponentPropsWithoutRef<typeof RadixDialog.Content> & { hideClose?: boolean }
>(({ className, children, hideClose, ...props }, ref) => (
    <DialogPortal>
        <DialogOverlay />
        <RadixDialog.Content
            ref={ref}
            className={cn(
                'fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2',
                'rounded-md bg-elevated p-6 shadow-popover',
                'data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0',
                'data-[state=open]:zoom-in-95 data-[state=closed]:zoom-out-95',
                className,
            )}
            {...props}
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
        </RadixDialog.Content>
    </DialogPortal>
));
DialogContent.displayName = 'DialogContent';

export const DialogHeader: React.FC<React.HTMLAttributes<HTMLDivElement>> = ({
    className,
    ...props
}) => <div className={cn('mb-3 flex flex-col gap-1', className)} {...props} />;

export const DialogTitle = forwardRef<
    React.ElementRef<typeof RadixDialog.Title>,
    React.ComponentPropsWithoutRef<typeof RadixDialog.Title>
>(({ className, ...props }, ref) => (
    <RadixDialog.Title
        ref={ref}
        className={cn('font-display text-md font-semibold text-text', className)}
        {...props}
    />
));
DialogTitle.displayName = 'DialogTitle';

export const DialogDescription = forwardRef<
    React.ElementRef<typeof RadixDialog.Description>,
    React.ComponentPropsWithoutRef<typeof RadixDialog.Description>
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
