// Tooltip 原子件。基于 Radix，受控/非受控两种模式都直接走 Radix 原型。
//
// 用法约定：
//   <TooltipProvider>  // 全局 1 个，挂在 AppNext 根上
//     <Tooltip>
//       <TooltipTrigger asChild><Button .../></TooltipTrigger>
//       <TooltipContent>查看日志</TooltipContent>
//     </Tooltip>
//   </TooltipProvider>
//
// BotCard 上每个 icon button (start / stop / logs / webui / settings) 都需要 tooltip
// 解释含义，是消费量最大的原子件之一。

import * as RadixTooltip from '@radix-ui/react-tooltip';
import { forwardRef } from 'react';
import { cn } from '../utils/cn';

export const TooltipProvider: React.FC<RadixTooltip.TooltipProviderProps> = ({
    delayDuration = 200,
    skipDelayDuration = 80,
    children,
    ...props
}) => (
    <RadixTooltip.Provider
        delayDuration={delayDuration}
        skipDelayDuration={skipDelayDuration}
        {...props}
    >
        {children}
    </RadixTooltip.Provider>
);

export const Tooltip = RadixTooltip.Root;
export const TooltipTrigger = RadixTooltip.Trigger;

export const TooltipContent = forwardRef<
    React.ElementRef<typeof RadixTooltip.Content>,
    React.ComponentPropsWithoutRef<typeof RadixTooltip.Content>
>(({ className, sideOffset = 6, ...props }, ref) => (
    <RadixTooltip.Portal>
        <RadixTooltip.Content
            ref={ref}
            sideOffset={sideOffset}
            className={cn(
                'z-50 max-w-xs rounded-sm bg-text px-2.5 py-1.5 text-2xs font-medium text-canvas shadow-popover',
                'data-[state=delayed-open]:animate-in data-[state=closed]:animate-out',
                'data-[state=delayed-open]:fade-in-0 data-[state=closed]:fade-out-0',
                className,
            )}
            {...props}
        />
    </RadixTooltip.Portal>
));
TooltipContent.displayName = 'TooltipContent';
