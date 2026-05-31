// Tabs 原子件。基于 Radix,A11y 由 Radix 兜底。第二轮重写。
//
// 改动:
//   1. 底部 brand 强调线从"每个 trigger 各自 scaleX"改成"单条线在不同 trigger 间
//      FLIP 滑动":TabsList 内挂一个独立 indicator span,active 切换时用 GSAP
//      把它从旧 trigger rect 滑到新 trigger rect。standard/rich 档启用,elegant
//      档退化为原来的 scaleX 渐变。
//   2. 内容切换:同一时刻只 mount 一个 content,但用 GsapPresence + 方向感
//      (新 tab 比旧 tab 索引大 → 从右滑入,反之从左)。
//   3. trigger 接入 m.bindHover 让悬停有 brightness 反馈,跟主题暖粉桃配色。

import * as RadixTabs from '@radix-ui/react-tabs';
import gsap from 'gsap';
import {
    createContext,
    forwardRef,
    useContext,
    useLayoutEffect,
    useRef,
    useState,
    type ReactNode,
} from 'react';
import { cn } from '../utils/cn';
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';
import { useMotion } from '../../hooks/preferences/useMotion';

interface TabsCtxValue {
    activeValue: string | undefined;
    /// 上一次 active 的 value。供 TabsContent 判断切换方向。
    prevActiveRef: React.MutableRefObject<string | undefined>;
    /// 提供"value 在 TabsList DOM 里的顺序索引"。listRef 由 TabsList 注册。
    listRef: React.MutableRefObject<HTMLElement | null>;
}
const TabsCtx = createContext<TabsCtxValue>({
    activeValue: undefined,
    prevActiveRef: { current: undefined },
    listRef: { current: null },
});

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
    const prevActiveRef = useRef<string | undefined>(actualValue);
    const listRef = useRef<HTMLElement | null>(null);
    return (
        <TabsCtx.Provider value={{ activeValue: actualValue, prevActiveRef, listRef }}>
            <RadixTabs.Root
                ref={ref}
                value={actualValue}
                onValueChange={handleChange}
                {...props}
            >
                {children}
            </RadixTabs.Root>
        </TabsCtx.Provider>
    );
});
Tabs.displayName = 'Tabs';

/// TabsList:在 Radix List 上叠加 FLIP indicator 层。
export const TabsList = forwardRef<
    React.ElementRef<typeof RadixTabs.List>,
    React.ComponentPropsWithoutRef<typeof RadixTabs.List>
>(({ className, children, ...props }, ref) => {
    const m = useMotion();
    const ctx = useContext(TabsCtx);
    const listRef = useRef<HTMLDivElement | null>(null);
    const indicatorRef = useRef<HTMLSpanElement | null>(null);

    const setListRef = (node: HTMLDivElement | null) => {
        listRef.current = node;
        ctx.listRef.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
    };

    // active 切换时把 indicator 从当前位置滑到新 trigger 的下沿。
    // standard/rich 档启用 FLIP;elegant 档把 indicator 直接 set 终态。
    useLayoutEffect(() => {
        const list = listRef.current;
        const indicator = indicatorRef.current;
        if (!list || !indicator) return;
        const active = list.querySelector<HTMLElement>(
            'button[role="tab"][data-state="active"]',
        );
        if (!active) {
            gsap.set(indicator, { autoAlpha: 0 });
            return;
        }
        const listRect = list.getBoundingClientRect();
        const activeRect = active.getBoundingClientRect();
        const left = activeRect.left - listRect.left + 12;
        const width = Math.max(activeRect.width - 24, 12);
        if (!m.enabled || !m.preset.feel.cardLift) {
            gsap.set(indicator, { autoAlpha: 1, x: left, width });
            return;
        }
        gsap.to(indicator, {
            autoAlpha: 1,
            x: left,
            width,
            duration: m.duration('base'),
            ease: m.ease.hover,
        });
    }, [ctx.activeValue, m.enabled, m.level, m.speed, m]);

    return (
        <RadixTabs.List
            ref={setListRef}
            className={cn(
                'relative inline-flex h-10 items-center gap-1 border-b border-border-subtle',
                className,
            )}
            {...props}
        >
            {children}
            <span
                ref={indicatorRef}
                aria-hidden
                style={{ visibility: 'hidden', opacity: 0 }}
                className="pointer-events-none absolute bottom-[-1px] left-0 h-0.5 rounded-pill bg-brand"
            />
        </RadixTabs.List>
    );
});
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

/// 读 TabsList DOM 里 trigger 的顺序,返回 value 的索引。-1 = 找不到。
function readTriggerIndex(listEl: HTMLElement | null, value: string): number {
    if (!listEl) return -1;
    const triggers = Array.from(
        listEl.querySelectorAll<HTMLElement>('button[role="tab"]'),
    );
    return triggers.findIndex((t) => t.getAttribute('value') === value || t.dataset.value === value);
}

export const TabsContent = forwardRef<
    React.ElementRef<typeof RadixTabs.Content>,
    TabsContentProps
>(({ className, value, children, ...props }, _ref) => {
    const ctx = useContext(TabsCtx);
    const isActive = ctx.activeValue === value;

    // 计算切换方向。每次 active 变化时:
    //   - 新 active 索引 > 旧 active 索引 → dir=1(从右滑入)
    //   - 反之 dir=-1
    // 索引读 DOM 顺序(Radix Trigger 会带 value 属性),不靠业务注册。
    const dirRef = useRef<1 | -1>(1);
    if (isActive) {
        const list = ctx.listRef.current;
        const newIdx = readTriggerIndex(list, value as string);
        const prev = ctx.prevActiveRef.current;
        const oldIdx = prev ? readTriggerIndex(list, prev) : -1;
        dirRef.current = oldIdx >= 0 && newIdx < oldIdx ? -1 : 1;
        // 写回 prev,供下次 active 切换计算方向。
        ctx.prevActiveRef.current = ctx.activeValue;
    }
    const dir = dirRef.current;

    const enter: EnterFn = (el, env) =>
        gsap.fromTo(
            el,
            { autoAlpha: 0, x: 12 * dir },
            {
                autoAlpha: 1,
                x: 0,
                duration: env.duration('base'),
                ease: env.ease.enter,
            },
        );
    const exit: ExitFn = (el, env) =>
        gsap.to(el, {
            autoAlpha: 0,
            x: -12 * dir,
            duration: env.duration('fast'),
            ease: env.ease.exit,
        });

    return (
        <GsapPresence visible={isActive} onEnter={enter} onExit={exit}>
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
