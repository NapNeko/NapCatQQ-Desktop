// Tabs 原子件。基于 Radix，A11y 由 Radix 兜底（焦点环 / 键盘箭头切换 / aria-selected）。
//
// 视觉走暖色控制台风：active tab 不用整块填充，只在底部画 2px brand 线，参考图气质一致。
// BotConfigPage 三个 tab 是它的主消费者；EventPanel 的 filter group 后续也可以复用。

import * as RadixTabs from '@radix-ui/react-tabs';
import { forwardRef } from 'react';
import { cn } from '../utils/cn';

export const Tabs = RadixTabs.Root;

export const TabsList = forwardRef<
    React.ElementRef<typeof RadixTabs.List>,
    React.ComponentPropsWithoutRef<typeof RadixTabs.List>
>(({ className, ...props }, ref) => (
    <RadixTabs.List
        ref={ref}
        className={cn(
            'inline-flex h-10 items-center gap-1 border-b border-border-subtle',
            className,
        )}
        {...props}
    />
));
TabsList.displayName = 'TabsList';

export const TabsTrigger = forwardRef<
    React.ElementRef<typeof RadixTabs.Trigger>,
    React.ComponentPropsWithoutRef<typeof RadixTabs.Trigger>
>(({ className, ...props }, ref) => (
    <RadixTabs.Trigger
        ref={ref}
        className={cn(
            'relative inline-flex h-10 items-center px-3 text-sm font-medium text-text-secondary transition-colors',
            'hover:text-text',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
            'data-[state=active]:text-text',
            // 底部强调线：active 状态下 2px brand 线浮于父 border 之上
            'data-[state=active]:after:absolute data-[state=active]:after:bottom-[-1px] data-[state=active]:after:left-0 data-[state=active]:after:right-0 data-[state=active]:after:h-0.5 data-[state=active]:after:bg-brand',
            'disabled:pointer-events-none disabled:opacity-50',
            className,
        )}
        {...props}
    />
));
TabsTrigger.displayName = 'TabsTrigger';

export const TabsContent = forwardRef<
    React.ElementRef<typeof RadixTabs.Content>,
    React.ComponentPropsWithoutRef<typeof RadixTabs.Content>
>(({ className, ...props }, ref) => (
    <RadixTabs.Content
        ref={ref}
        className={cn(
            'mt-4 focus-visible:outline-none',
            className,
        )}
        {...props}
    />
));
TabsContent.displayName = 'TabsContent';
