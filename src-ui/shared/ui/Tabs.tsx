// Tabs 原子件。基于 Radix,A11y 由 Radix 兜底。
//
// 视觉:active 走暖色 + 底部 2px brand 线 scaleX 渐变。
// 内容切换:Tabs Root 维护 ActiveValueContext,TabsContent 读 context 决定 visible,
// 用 GsapPresence 跑 fade + slide-x 进退场。同一时刻只 mount 一个 content。

import * as RadixTabs from '@radix-ui/react-tabs';
import gsap from 'gsap';
import {
    createContext,
    forwardRef,
    useContext,
    useState,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';

const ActiveValueContext = createContext<string | undefined>(undefined);

interface TabsProps
    extends React.ComponentPropsWithoutRef<typeof RadixTabs.Root> {}

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
            // 底部强调线:scaleX 0→1 渐变。
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

const contentEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, x: 12 },
        {
            autoAlpha: 1,
            x: 0,
            duration: env.duration('base'),
            ease: env.preset.enterEase,
        },
    );
const contentExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        x: -12,
        duration: env.duration('fast'),
        ease: env.preset.exitEase,
    });

interface TabsContentProps
    extends Omit<
        React.ComponentPropsWithoutRef<typeof RadixTabs.Content>,
        'forceMount'
    > {
    children?: ReactNode;
}

export const TabsContent = forwardRef<
    React.ElementRef<typeof RadixTabs.Content>,
    TabsContentProps
>(({ className, value, children, ...props }, _ref) => {
    const activeValue = useContext(ActiveValueContext);
    const isActive = activeValue === value;

    return (
        <GsapPresence visible={isActive} onEnter={contentEnter} onExit={contentExit}>
            <RadixTabs.Content value={value} asChild forceMount {...props}>
                <ContentBody className={className}>{children}</ContentBody>
            </RadixTabs.Content>
        </GsapPresence>
    );
});
TabsContent.displayName = 'TabsContent';

const ContentBody = forwardRef<
    HTMLDivElement,
    { className?: string; children?: ReactNode }
>(({ className, children }, ref) => (
    <div
        ref={ref}
        style={{ visibility: 'hidden', opacity: 0 }}
        className={cn('mt-4 focus-visible:outline-none', className)}
    >
        {children}
    </div>
));
ContentBody.displayName = 'TabsContentBody';
