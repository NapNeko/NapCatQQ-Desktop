// Tabs 原子件。基于 Radix,A11y 由 Radix 兜底(焦点环 / 键盘箭头切换 / aria-selected)。
//
// 视觉走暖色控制台风:active tab 不用整块填充,只在底部画 2px brand 线,参考图气质一致。
// BotConfigPage 三个 tab 是它的主消费者;EventPanel 的 filter group 后续也可以复用。
//
// 动画:
//   - trigger 下划线:scaleX 0→1 渐变,Radix data-state 驱动,无需 layoutId
//   - content 切换:Tabs Root 包一层 ActiveValueContext,TabsContent 读 context 自己决定
//     mount,套 AnimatePresence mode="wait" 让旧 tab 退场再进新 tab。这样切换不再是
//     瞬变,而是横向滑动 + fade。

import * as RadixTabs from '@radix-ui/react-tabs';
import { AnimatePresence, motion } from 'framer-motion';
import {
    createContext,
    forwardRef,
    useContext,
    useState,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

const ActiveValueContext = createContext<string | undefined>(undefined);

interface TabsProps
    extends React.ComponentPropsWithoutRef<typeof RadixTabs.Root> {}

/// Tabs wrapper:对外接口跟 RadixTabs.Root 完全一致,内部多走一层 context
/// 把 active value 传给 TabsContent。受控/非受控都支持。
export const Tabs = forwardRef<
    React.ElementRef<typeof RadixTabs.Root>,
    TabsProps
>(({ value, defaultValue, onValueChange, children, ...props }, ref) => {
    const isControlled = value !== undefined;
    const [internal, setInternal] = useState<string | undefined>(defaultValue);
    const actualValue = isControlled ? value : internal;
    const handleChange = (next: string) => {
        if (!isControlled) setInternal(next);
        onValueChange?.(next);
    };
    return (
        <ActiveValueContext.Provider value={actualValue}>
            <RadixTabs.Root
                ref={ref}
                value={actualValue}
                onValueChange={handleChange}
                {...props}
            >
                {children}
            </RadixTabs.Root>
        </ActiveValueContext.Provider>
    );
});
Tabs.displayName = 'Tabs';

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
            'relative inline-flex h-10 items-center px-3 text-sm font-medium text-text-secondary',
            'transition-colors duration-200 ease-out',
            'hover:text-text',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
            'data-[state=active]:text-text',
            // 底部强调线:用伪元素 scaleX 0→1 渐变,避免直接 width 触发 layout。
            'after:absolute after:bottom-[-1px] after:left-3 after:right-3 after:h-0.5 after:rounded-pill after:bg-brand',
            'after:origin-center after:scale-x-0 after:transition-transform after:duration-300 after:ease-out',
            'data-[state=active]:after:scale-x-100',
            'disabled:pointer-events-none disabled:opacity-50',
            className,
        )}
        {...props}
    />
));
TabsTrigger.displayName = 'TabsTrigger';

interface TabsContentProps
    extends Omit<
        React.ComponentPropsWithoutRef<typeof RadixTabs.Content>,
        'forceMount'
    > {
    children?: ReactNode;
}

/// 自管 mount + framer 进退场。Radix Content 用 forceMount 让我们持有节点,
/// 外层 AnimatePresence 根据 active value 决定渲染哪个 tab。同一时刻只渲染
/// 一个,mode="wait" 让旧的退完再进新的,避免左右两个 content 重叠。
export const TabsContent = forwardRef<
    React.ElementRef<typeof RadixTabs.Content>,
    TabsContentProps
>(({ className, value, children, ...props }, ref) => {
    const activeValue = useContext(ActiveValueContext);
    const m = useMotion();
    const isActive = activeValue === value;

    return (
        <AnimatePresence mode="wait" initial={false}>
            {isActive && (
                <RadixTabs.Content
                    ref={ref}
                    value={value}
                    asChild
                    forceMount
                    {...props}
                >
                    <motion.div
                        // 横向滑入 + fade。每次 active 切换,key=value 让 framer
                        // 把旧节点 unmount + 新节点 mount,各自跑 enter/exit。
                        key={value}
                        initial={{ opacity: 0, x: 12 }}
                        animate={{ opacity: 1, x: 0 }}
                        exit={{ opacity: 0, x: -12, transition: { duration: 0.14 } }}
                        transition={m.transition('base')}
                        className={cn(
                            'mt-4 focus-visible:outline-none',
                            className,
                        )}
                    >
                        {children}
                    </motion.div>
                </RadixTabs.Content>
            )}
        </AnimatePresence>
    );
});
TabsContent.displayName = 'TabsContent';
