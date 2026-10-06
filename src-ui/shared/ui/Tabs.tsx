// Tabs 原子件。基于 Radix,A11y 由 Radix 兜底。
//
// 改动:
//   1. 底部 brand 强调线从"每个 trigger 各自 scaleX"改成"单条线在不同 trigger 间
//      FLIP 滑动":TabsList 内挂一个独立 indicator span,active 切换时用 GSAP
//      把它从旧 trigger rect 滑到新 trigger rect。standard/rich 档启用,elegant
//      档退化为原来的 scaleX 渐变。
//   2. 内容切换:Radix 直接管 mount/unmount,新 content mount 时只跑 fade-in
//      (不做横向 slide;长内容 tab 切换时双份 content 短暂并存会让滚动条/高度
//      错位,看起来"不衔接"。方向感留给 PageTransition,Tab 这种密集切换上不需要)。

import * as RadixTabs from '@radix-ui/react-tabs';
import { useGSAP } from '@gsap/react';
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

gsap.registerPlugin(useGSAP);
import { cn } from '../utils/cn';
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

interface TabsProps extends React.ComponentPropsWithoutRef<typeof RadixTabs.Root> {}

export const Tabs = forwardRef<React.ElementRef<typeof RadixTabs.Root>, TabsProps>(
    ({ value, defaultValue, onValueChange, children, ...props }, ref) => {
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
    },
);
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
        const active = list.querySelector<HTMLElement>('button[role="tab"][data-state="active"]');
        if (!active) {
            gsap.set(indicator, { autoAlpha: 0 });
            return;
        }
        const listRect = list.getBoundingClientRect();
        const activeRect = active.getBoundingClientRect();
        const left = activeRect.left - listRect.left + 12;
        const width = Math.max(activeRect.width - 24, 12);
        if (!m.enabled) {
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

/// 竖排导航（详情页侧栏）。和主侧栏一套语言：选中行左侧一根品牌竖条，在行间 FLIP 滑动；
/// 这里没有图标，所以选中行再垫一层淡品牌底。Tabs 根上记得配 orientation="vertical"，上下键才对。
/// 列表里可以夹分组标题之类的非 trigger 元素，roving focus 只在 trigger 之间走。
export const TabsSideList = forwardRef<
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

    useLayoutEffect(() => {
        const list = listRef.current;
        const indicator = indicatorRef.current;
        if (!list || !indicator) return;
        const active = list.querySelector<HTMLElement>('button[role="tab"][data-state="active"]');
        if (!active) {
            gsap.set(indicator, { autoAlpha: 0 });
            return;
        }
        const listRect = list.getBoundingClientRect();
        const activeRect = active.getBoundingClientRect();
        const y = activeRect.top - listRect.top + 7;
        const height = Math.max(activeRect.height - 14, 8);
        if (!m.enabled) {
            gsap.set(indicator, { autoAlpha: 1, y, height });
            return;
        }
        gsap.to(indicator, {
            autoAlpha: 1,
            y,
            height,
            duration: m.duration('base'),
            ease: m.ease.hover,
        });
    }, [ctx.activeValue, m.enabled, m.level, m.speed, m]);

    return (
        <RadixTabs.List
            ref={setListRef}
            className={cn('relative flex flex-col gap-0.5', className)}
            {...props}
        >
            {children}
            <span
                ref={indicatorRef}
                aria-hidden
                style={{ visibility: 'hidden', opacity: 0 }}
                className="pointer-events-none absolute left-0 top-0 w-[2px] rounded-r-pill bg-brand"
            />
        </RadixTabs.List>
    );
});
TabsSideList.displayName = 'TabsSideList';

export type TabsSideDot = 'brand' | 'warning' | 'danger';

const SIDE_DOT_LABEL: Record<TabsSideDot, string> = {
    brand: '下一步在这里',
    warning: '有设置要留意',
    danger: '这页有填错的地方',
};

// 品牌橙和危险红在 6px 上几乎分不出来，所以「下一步」画空心环，「有问题」画实心点，靠形状区分
const SIDE_DOT_CLASS: Record<TabsSideDot, string> = {
    brand: 'h-[7px] w-[7px] border-[1.5px] border-brand',
    warning: 'h-1.5 w-1.5 bg-warning',
    danger: 'h-1.5 w-1.5 bg-danger',
};

/// dot：右侧小圆点。brand 表示下一步该来这页，warning 表示这页有配置冲突，danger 表示这页有填错的字段。
export const TabsSideTrigger = forwardRef<
    React.ElementRef<typeof RadixTabs.Trigger>,
    React.ComponentPropsWithoutRef<typeof RadixTabs.Trigger> & { dot?: TabsSideDot }
>(({ className, children, dot, ...props }, ref) => (
    <RadixTabs.Trigger
        ref={ref}
        className={cn(
            'flex h-8 w-full shrink-0 items-center justify-between gap-2 rounded-sm pl-3 pr-2 text-left text-[13px] text-text-secondary',
            'transition-colors duration-150 ease-out',
            'hover:bg-text/5 hover:text-text',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
            'data-[state=active]:bg-brand-soft/60 data-[state=active]:font-medium data-[state=active]:text-text',
            'disabled:pointer-events-none disabled:opacity-50',
            className,
        )}
        {...props}
    >
        <span className="truncate">{children}</span>
        {dot && (
            <>
                <span aria-hidden className={cn('shrink-0 rounded-full', SIDE_DOT_CLASS[dot])} />
                <span className="sr-only">{SIDE_DOT_LABEL[dot]}</span>
            </>
        )}
    </RadixTabs.Trigger>
));
TabsSideTrigger.displayName = 'TabsSideTrigger';

interface TabsContentProps extends Omit<
    React.ComponentPropsWithoutRef<typeof RadixTabs.Content>,
    'forceMount'
> {
    children?: ReactNode;
}

/// TabsContent:走纯 fade-in,不做"旧 exit 完再换新"那一套。
///
/// 原因:Radix Tabs 在 active 切换时会立即让旧 content unmount + 新 content mount。
/// 我们之前包 GsapPresence + 横向 slide,会导致两份内容短暂并存(旧的还在跑 exit,
/// 新的已经 enter),长内容(如 Settings 长列表)会出现一瞬间高度叠加 + 滚动条
/// 重排,看起来"卡卡的不衔接"。
///
/// 现在改成:让 Radix 直接管 mount/unmount,只在新 content mount 时给一次 GSAP
/// fade-in。视觉上是干脆的"消失 → 淡入",没有横向 slide / 方向感(方向感留给
/// PageTransition;在 Tab 这种密集切换上方向感反而干扰阅读)。

export const TabsContent = forwardRef<React.ElementRef<typeof RadixTabs.Content>, TabsContentProps>(
    ({ className, value, children, ...props }, _ref) => {
        const ctx = useContext(TabsCtx);
        const isActive = ctx.activeValue === value;
        const m = useMotion();

        // 写 prev 给 TabsList indicator 用(它读 prevActiveRef 算方向时也会用到)。
        if (isActive && ctx.prevActiveRef.current !== ctx.activeValue) {
            ctx.prevActiveRef.current = ctx.activeValue;
        }

        if (!isActive) return null;

        return (
            <RadixTabs.Content value={value} {...props} asChild>
                <ContentBody contentKey={value} m={m} className={className}>
                    {children}
                </ContentBody>
            </RadixTabs.Content>
        );
    },
);
TabsContent.displayName = 'TabsContent';

const ContentBody = forwardRef<
    HTMLDivElement,
    {
        className?: string;
        children?: ReactNode;
        contentKey: string;
        m: ReturnType<typeof useMotion>;
    }
>(({ className, children, contentKey, m }, ref) => {
    const localRef = useRef<HTMLDivElement | null>(null);
    const setRef = (node: HTMLDivElement | null) => {
        localRef.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
    };

    // 与 DialogStepTransition 同节奏：淡入 + 轻微上移；不做横向 slide / 双份 mount，
    // 避免 Settings 长列表切换时高度叠加。
    useGSAP(
        () => {
            const el = localRef.current;
            if (!el) return;
            gsap.killTweensOf(el);
            if (!m.enabled) {
                gsap.set(el, { autoAlpha: 1, y: 0 });
                return;
            }
            gsap.fromTo(
                el,
                { autoAlpha: 0, y: 8 },
                {
                    autoAlpha: 1,
                    y: 0,
                    duration: m.duration('base'),
                    ease: m.ease.enter,
                    clearProps: 'transform',
                },
            );
        },
        {
            dependencies: [contentKey, m.enabled, m.level, m.speed],
            scope: localRef,
        },
    );

    return (
        <div ref={setRef} className={cn('mt-4 focus-visible:outline-none', className)}>
            {children}
        </div>
    );
});
ContentBody.displayName = 'TabsContentBody';
