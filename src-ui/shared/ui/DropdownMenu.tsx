// 下拉菜单原子件。基于 Radix Dropdown Menu，视觉与 ContextMenu 同一套 token。

import * as React from 'react';
import * as RadixDropdownMenu from '@radix-ui/react-dropdown-menu';
import { Check } from 'lucide-react';
import { cn } from '../utils/cn';

// 默认非模态：模态版会给 body 加 pointer-events:none，和 Dialog 叠用时容易卡住点击。
export function DropdownMenu({
    modal = false,
    ...props
}: React.ComponentPropsWithoutRef<typeof RadixDropdownMenu.Root>) {
    return <RadixDropdownMenu.Root modal={modal} {...props} />;
}

export const DropdownMenuTrigger = RadixDropdownMenu.Trigger;
export const DropdownMenuGroup = RadixDropdownMenu.Group;
export const DropdownMenuRadioGroup = RadixDropdownMenu.RadioGroup;

export const DropdownMenuContent = React.forwardRef<
    React.ElementRef<typeof RadixDropdownMenu.Content>,
    React.ComponentPropsWithoutRef<typeof RadixDropdownMenu.Content>
>(({ className, sideOffset = 6, collisionPadding = 8, ...props }, ref) => (
    <RadixDropdownMenu.Portal>
        <RadixDropdownMenu.Content
            ref={ref}
            sideOffset={sideOffset}
            collisionPadding={collisionPadding}
            className={cn(
                'z-50 min-w-[180px] max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-y-auto rounded-md border border-border-subtle/80 bg-elevated/95 p-1 text-text shadow-popover backdrop-blur-md',
                className,
            )}
            {...props}
        />
    </RadixDropdownMenu.Portal>
));
DropdownMenuContent.displayName = RadixDropdownMenu.Content.displayName;

export interface DropdownMenuItemProps extends React.ComponentPropsWithoutRef<
    typeof RadixDropdownMenu.Item
> {
    tone?: 'default' | 'danger';
}

const itemClass =
    'relative flex cursor-pointer select-none items-center gap-2 rounded-sm px-2.5 py-1.5 text-xs outline-none transition-colors data-[disabled]:pointer-events-none data-[disabled]:opacity-40';

export const DropdownMenuItem = React.forwardRef<
    React.ElementRef<typeof RadixDropdownMenu.Item>,
    DropdownMenuItemProps
>(({ className, tone = 'default', ...props }, ref) => (
    <RadixDropdownMenu.Item
        ref={ref}
        className={cn(
            itemClass,
            tone === 'default' && 'text-text data-[highlighted]:bg-field',
            tone === 'danger' && 'text-danger data-[highlighted]:bg-danger-soft/40',
            className,
        )}
        {...props}
    />
));
DropdownMenuItem.displayName = RadixDropdownMenu.Item.displayName;

export const DropdownMenuRadioItem = React.forwardRef<
    React.ElementRef<typeof RadixDropdownMenu.RadioItem>,
    React.ComponentPropsWithoutRef<typeof RadixDropdownMenu.RadioItem>
>(({ className, children, ...props }, ref) => (
    <RadixDropdownMenu.RadioItem
        ref={ref}
        className={cn(
            itemClass,
            'pr-8 text-text data-[highlighted]:bg-field data-[state=checked]:text-brand',
            className,
        )}
        {...props}
    >
        {children}
        <span className="absolute right-2.5 flex h-3.5 w-3.5 items-center justify-center">
            <RadixDropdownMenu.ItemIndicator>
                <Check size={13} className="stroke-[2.5]" />
            </RadixDropdownMenu.ItemIndicator>
        </span>
    </RadixDropdownMenu.RadioItem>
));
DropdownMenuRadioItem.displayName = RadixDropdownMenu.RadioItem.displayName;

export const DropdownMenuLabel = React.forwardRef<
    React.ElementRef<typeof RadixDropdownMenu.Label>,
    React.ComponentPropsWithoutRef<typeof RadixDropdownMenu.Label>
>(({ className, ...props }, ref) => (
    <RadixDropdownMenu.Label
        ref={ref}
        className={cn('px-2.5 pb-1 pt-1.5 text-2xs font-medium text-text-tertiary', className)}
        {...props}
    />
));
DropdownMenuLabel.displayName = RadixDropdownMenu.Label.displayName;

export const DropdownMenuSeparator = React.forwardRef<
    React.ElementRef<typeof RadixDropdownMenu.Separator>,
    React.ComponentPropsWithoutRef<typeof RadixDropdownMenu.Separator>
>(({ className, ...props }, ref) => (
    <RadixDropdownMenu.Separator
        ref={ref}
        className={cn('-mx-1 my-1 h-px bg-border-subtle', className)}
        {...props}
    />
));
DropdownMenuSeparator.displayName = RadixDropdownMenu.Separator.displayName;
