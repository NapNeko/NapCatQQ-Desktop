// 上下文右键菜单原子件。基于 Radix Context Menu。
//
// 视觉语言与 Popover/DropdownMenu 对齐：
//   - 背景：bg-elevated/95 + backdrop-blur-md
//   - 阴影：shadow-popover
//   - 边框：border-border-subtle/80
//   - 选项：rounded-sm + hover/focus 高亮
//   - 语义色：danger（删除/停止）/ brand（推荐/主操作）

import * as React from 'react';
import * as RadixContextMenu from '@radix-ui/react-context-menu';
import { Check, ChevronRight, Circle } from 'lucide-react';
import { cn } from '../utils/cn';

export function ContextMenu({
    modal = false,
    ...props
}: React.ComponentPropsWithoutRef<typeof RadixContextMenu.Root>) {
    return <RadixContextMenu.Root modal={modal} {...props} />;
}

export const ContextMenuTrigger = RadixContextMenu.Trigger;
export const ContextMenuGroup = RadixContextMenu.Group;
export const ContextMenuPortal = RadixContextMenu.Portal;
export const ContextMenuSub = RadixContextMenu.Sub;
export const ContextMenuRadioGroup = RadixContextMenu.RadioGroup;

export const ContextMenuSubTrigger = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.SubTrigger>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.SubTrigger> & {
        inset?: boolean;
    }
>(({ className, inset, children, ...props }, ref) => (
    <RadixContextMenu.SubTrigger
        ref={ref}
        className={cn(
            'flex cursor-default select-none items-center rounded-sm px-2.5 py-1.5 text-xs text-text outline-none transition-colors focus:bg-field focus:text-text data-[state=open]:bg-field',
            inset && 'pl-8',
            className,
        )}
        {...props}
    >
        {children}
        <ChevronRight size={13} className="ml-auto text-text-tertiary" />
    </RadixContextMenu.SubTrigger>
));
ContextMenuSubTrigger.displayName = RadixContextMenu.SubTrigger.displayName;

export const ContextMenuSubContent = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.SubContent>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.SubContent>
>(({ className, collisionPadding = 8, style, ...props }, ref) => (
    <RadixContextMenu.Portal>
        <RadixContextMenu.SubContent
            ref={ref}
            collisionPadding={collisionPadding}
            style={{
                transformOrigin: 'var(--radix-context-menu-content-transform-origin)',
                ...style,
            }}
            className={cn(
                'z-50 min-w-[160px] overflow-hidden rounded-md border border-border-subtle/80 bg-elevated/95 p-1 text-text shadow-popover backdrop-blur-md',
                'animate-in fade-in-0 duration-100',
                className,
            )}
            {...props}
        />
    </RadixContextMenu.Portal>
));
ContextMenuSubContent.displayName = RadixContextMenu.SubContent.displayName;

export const ContextMenuContent = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.Content>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.Content>
>(({ className, collisionPadding = 8, style, ...props }, ref) => (
    <RadixContextMenu.Portal>
        <RadixContextMenu.Content
            ref={ref}
            collisionPadding={collisionPadding}
            style={{
                transformOrigin: 'var(--radix-context-menu-content-transform-origin)',
                ...style,
            }}
            className={cn(
                'z-50 min-w-[180px] overflow-hidden rounded-md border border-border-subtle/80 bg-elevated/95 p-1 text-text shadow-popover backdrop-blur-md',
                'animate-in fade-in-0 duration-100',
                className,
            )}
            {...props}
        />
    </RadixContextMenu.Portal>
));
ContextMenuContent.displayName = RadixContextMenu.Content.displayName;

export interface ContextMenuItemProps
    extends React.ComponentPropsWithoutRef<typeof RadixContextMenu.Item> {
    inset?: boolean;
    tone?: 'default' | 'danger' | 'brand' | 'success';
}

export const ContextMenuItem = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.Item>,
    ContextMenuItemProps
>(({ className, inset, tone = 'default', ...props }, ref) => (
    <RadixContextMenu.Item
        ref={ref}
        className={cn(
            'relative flex cursor-pointer select-none items-center gap-2 rounded-sm px-2.5 py-1.5 text-xs outline-none transition-colors data-[disabled]:pointer-events-none data-[disabled]:opacity-40',
            tone === 'default' &&
            'text-text focus:bg-field focus:text-text data-[highlighted]:bg-field data-[highlighted]:text-text',
            tone === 'danger' &&
            'text-danger focus:bg-danger-soft/40 focus:text-danger data-[highlighted]:bg-danger-soft/40 data-[highlighted]:text-danger',
            tone === 'brand' &&
            'text-brand focus:bg-brand/10 focus:text-brand data-[highlighted]:bg-brand/10 data-[highlighted]:text-brand',
            tone === 'success' &&
            'text-success focus:bg-success-soft/40 focus:text-success data-[highlighted]:bg-success-soft/40 data-[highlighted]:text-success',
            inset && 'pl-8',
            className,
        )}
        {...props}
    />
));
ContextMenuItem.displayName = RadixContextMenu.Item.displayName;

export const ContextMenuCheckboxItem = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.CheckboxItem>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.CheckboxItem>
>(({ className, children, checked, ...props }, ref) => (
    <RadixContextMenu.CheckboxItem
        ref={ref}
        className={cn(
            'relative flex cursor-pointer select-none items-center rounded-sm py-1.5 pl-8 pr-2.5 text-xs text-text outline-none transition-colors focus:bg-field focus:text-text data-[disabled]:pointer-events-none data-[disabled]:opacity-40 data-[highlighted]:bg-field data-[highlighted]:text-text',
            className,
        )}
        checked={checked}
        {...props}
    >
        <span className="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
            <RadixContextMenu.ItemIndicator>
                <Check size={13} className="text-brand stroke-[2.5]" />
            </RadixContextMenu.ItemIndicator>
        </span>
        {children}
    </RadixContextMenu.CheckboxItem>
));
ContextMenuCheckboxItem.displayName = RadixContextMenu.CheckboxItem.displayName;

export const ContextMenuRadioItem = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.RadioItem>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.RadioItem>
>(({ className, children, ...props }, ref) => (
    <RadixContextMenu.RadioItem
        ref={ref}
        className={cn(
            'relative flex cursor-pointer select-none items-center rounded-sm py-1.5 pl-8 pr-2.5 text-xs text-text outline-none transition-colors focus:bg-field focus:text-text data-[disabled]:pointer-events-none data-[disabled]:opacity-40 data-[highlighted]:bg-field data-[highlighted]:text-text',
            className,
        )}
        {...props}
    >
        <span className="absolute left-2.5 flex h-3.5 w-3.5 items-center justify-center">
            <RadixContextMenu.ItemIndicator>
                <Circle size={6} className="fill-brand text-brand" />
            </RadixContextMenu.ItemIndicator>
        </span>
        {children}
    </RadixContextMenu.RadioItem>
));
ContextMenuRadioItem.displayName = RadixContextMenu.RadioItem.displayName;

export const ContextMenuLabel = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.Label>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.Label> & {
        inset?: boolean;
    }
>(({ className, inset, ...props }, ref) => (
    <RadixContextMenu.Label
        ref={ref}
        className={cn(
            'px-2.5 py-1.5 text-[11px] font-semibold text-text-tertiary',
            inset && 'pl-8',
            className,
        )}
        {...props}
    />
));
ContextMenuLabel.displayName = RadixContextMenu.Label.displayName;

export const ContextMenuSeparator = React.forwardRef<
    React.ElementRef<typeof RadixContextMenu.Separator>,
    React.ComponentPropsWithoutRef<typeof RadixContextMenu.Separator>
>(({ className, ...props }, ref) => (
    <RadixContextMenu.Separator
        ref={ref}
        className={cn('-mx-1 my-1 h-px bg-border-subtle/60', className)}
        {...props}
    />
));
ContextMenuSeparator.displayName = RadixContextMenu.Separator.displayName;

export const ContextMenuShortcut: React.FC<React.HTMLAttributes<HTMLSpanElement>> = ({
    className,
    ...props
}) => {
    return (
        <span
            className={cn(
                'ml-auto font-mono text-[10px] tracking-wider text-text-tertiary',
                className,
            )}
            {...props}
        />
    );
};
ContextMenuShortcut.displayName = 'ContextMenuShortcut';

