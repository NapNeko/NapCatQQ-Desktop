// 调试台三栏的外框：栏本身（错误边界 + 定高的内容区）、栏间可拖的分隔条、左栏收起后的窄边。
//
// 内容区是 `flex min-h-0 flex-1 flex-col overflow-hidden`：高度来自页面，不随内容长，
// 列表、JSON 树、表格都在里面自己滚，整页永远不滚。

import {
    forwardRef,
    useCallback,
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
    type CSSProperties,
    type KeyboardEvent as ReactKeyboardEvent,
    type PointerEvent as ReactPointerEvent,
    type ReactNode,
    type RefObject,
} from 'react';
import { PanelLeftOpen } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../shared/ui';
import { ActionMotionIcon } from '../../shared/ui/motion';
import { RouteErrorBoundary } from '../../shared/ui/RouteErrorBoundary';
import { useMotion } from '../../hooks/preferences/useMotion';
import { cssEase } from '../../core/design/cssEase';
import { KEYBOARD_RESIZE_STEP, type ColumnSide } from '../../core/domain/debug/workbenchLayout';
import { LEFT_PANELS, type DebugLeftPanel } from './leftPanels';

// ---------------------------------------------------------------------------
// 栏
// ---------------------------------------------------------------------------

export interface ColumnFrameProps {
    /** 栏名：给屏幕阅读器（region 的名字） */
    title: string;
    /** 这一栏渲染出错时错误边界上的标题 */
    errorTitle: string;
    /** 固定宽度（px）；不给就占满剩下的（中栏） */
    width?: number;
    /** 标题行；不给就没有（中栏、右栏的标题行由各自内容自己画） */
    header?: ReactNode;
    /** 标题行下面的一条提示，比如「Bot 没在运行」 */
    notice?: ReactNode;
    /**
     * 用户刚点了展开时，内容从哪边滑进来。进页面时的首帧不传——整页已经有路由切换动画了。
     * 宽度是瞬间到位的，只有内容走 transform / opacity。
     */
    appearFrom?: ColumnSide | null;
    className?: string;
    children: ReactNode;
}

export const ColumnFrame = forwardRef<HTMLElement, ColumnFrameProps>(function ColumnFrame(
    { title, errorTitle, width, header, notice, appearFrom = null, className, children },
    ref,
) {
    const m = useMotion();
    const bodyRef = useRef<HTMLDivElement>(null);
    // 只在挂上那一刻看一眼：之后的重渲不该重播
    const appearRef = useRef(appearFrom);

    useLayoutEffect(() => {
        const el = bodyRef.current;
        const from = appearRef.current;
        if (!el || !from || !m.enabled || typeof el.animate !== 'function') return;
        const dx = from === 'left' ? -12 : 12;
        const anim = el.animate(
            [
                { opacity: 0, transform: `translateX(${dx}px)` },
                { opacity: 1, transform: 'none' },
            ],
            { duration: m.duration('base') * 1000, easing: cssEase(m.ease.enter) },
        );
        return () => anim.cancel();
        // 只在挂载时播一次
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const style: CSSProperties | undefined = width === undefined ? undefined : { width };

    return (
        <section
            ref={ref}
            aria-label={title}
            style={style}
            className={cn(
                'flex min-h-0 min-w-0 flex-col',
                width === undefined ? 'flex-1' : 'shrink-0',
                className,
            )}
        >
            {header}
            {notice}
            <div ref={bodyRef} data-column-body="" className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
                <RouteErrorBoundary title={errorTitle}>{children}</RouteErrorBoundary>
            </div>
        </section>
    );
});

/**
 * 旁边一栏收起 / 展开时，这一栏的左边缘一下子跳出一两百像素。宽度照样瞬间到位（不动 width），
 * 内容从原来的位置滑到新位置：只动 transform，被这一栏自己的内容区裁住，不会盖到别的栏上。
 * 用法：改布局之前调一下返回的函数记下位置，`shiftKey` 变了之后自动补一段滑动；关了动画时什么都不做。
 */
// 内容节点 = [data-column-body] 的第一个元素子节点。用 firstElementChild 而不是
// querySelector('[data-column-body] > *')：桌面端两者相同，但部分 jsdom/nwsapi 版本
// 对「属性选择器 + > + *」的匹配有 bug（> div 正常、> * 落空），测试环境会查不到。
function columnBodyChild(section: HTMLElement): HTMLElement | null {
    return (section.querySelector('[data-column-body]')?.firstElementChild as HTMLElement | null) ?? null;
}

export function useSlideAfterShift(sectionRef: RefObject<HTMLElement | null>, shiftKey: unknown): () => void {
    const m = useMotion();
    // 记的是内容节点带 transform 的视觉左缘，不是 section 的位置：
    // section 本身不动，滑动放在内容上。收起滑到一半立刻展开时，量 section 会丢掉正在跑的
    // transform，新动画从布局起点开始，内容先往回跳一段再滑；量内容才能拿到此刻的真实位置。
    const from = useRef<number | null>(null);
    const animRef = useRef<Animation | null>(null);
    useLayoutEffect(() => {
        // 上一段可能还在跑：先停掉再量，内容回到布局位置，量出来的才是这一段的真实起点
        animRef.current?.cancel();
        animRef.current = null;
        const start = from.current;
        from.current = null;
        const section = sectionRef.current;
        if (start === null || !section || !m.enabled) return;
        const content = columnBodyChild(section);
        if (!content || typeof content.animate !== 'function') return;
        const dx = Math.round(start - content.getBoundingClientRect().left);
        if (Math.abs(dx) < 1) return;
        const anim = content.animate([{ transform: `translateX(${dx}px)` }, { transform: 'none' }], {
            duration: m.duration('base') * 1000,
            easing: cssEase(m.ease.enter),
        });
        animRef.current = anim;
        return () => {
            anim.cancel();
            if (animRef.current === anim) animRef.current = null;
        };
        // 只在布局真的换了（shiftKey 变了）之后播
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [shiftKey]);
    return useCallback(() => {
        const section = sectionRef.current;
        from.current = (section && columnBodyChild(section)?.getBoundingClientRect().left) ?? null;
    }, [sectionRef]);
}

/** 栏顶标题行的统一样式：三栏各自的标题行都用这个高度和底边，横着看是一条线 */
export const COLUMN_HEADER_CLASS = 'flex h-10 shrink-0 items-center gap-1.5 border-b border-border-subtle/70 px-2';

// ---------------------------------------------------------------------------
// 分隔条
// ---------------------------------------------------------------------------

export interface ColumnSplitterProps {
    /** 调哪一栏：左栏往右拖变宽，右栏往左拖变宽 */
    side: ColumnSide;
    label: string;
    /** 这一栏眼下画出来的宽度 */
    value: number;
    /** 按下那一刻算一次允许的范围（窗口宽度、另一栏的宽度都可能变） */
    getBounds: () => { min: number; max: number };
    /** 拖动过程中每帧调一次；调用方直接改 DOM 宽度，不走 React，拖起来不会整页重渲 */
    onPreview: (width: number) => void;
    /** 松手（或键盘调整）时调一次，落盘 */
    onCommit: (width: number) => void;
    /** 双击恢复默认宽度 */
    onReset: () => void;
}

export function ColumnSplitter({ side, label, value, getBounds, onPreview, onCommit, onReset }: ColumnSplitterProps) {
    const [dragging, setDragging] = useState(false);
    const drag = useRef<{
        pointerId: number;
        startX: number;
        startWidth: number;
        bounds: { min: number; max: number };
        width: number;
        frame: number;
    } | null>(null);
    const bounds = getBounds();

    // 拖到一半组件没了（切路由、收起了这一栏）：别把 body 的光标和选区样式留下
    useEffect(() => () => endDragStyles(), []);

    const finish = (commit: boolean) => {
        const d = drag.current;
        if (!d) return;
        drag.current = null;
        if (d.frame) cancelAnimationFrame(d.frame);
        endDragStyles();
        setDragging(false);
        // 最后一帧可能还没画上（拖出去又在同一帧里拖回原宽）：DOM 宽度无论如何都对一遍
        const width = commit ? d.width : d.startWidth;
        onPreview(width);
        if (width !== d.startWidth) onCommit(width);
    };

    const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        drag.current = {
            pointerId: e.pointerId,
            startX: e.clientX,
            startWidth: value,
            bounds: getBounds(),
            width: value,
            frame: 0,
        };
        startDragStyles();
        setDragging(true);
    };

    const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
        const d = drag.current;
        if (!d || e.pointerId !== d.pointerId) return;
        const dx = e.clientX - d.startX;
        const next = clamp(d.startWidth + (side === 'left' ? dx : -dx), d.bounds);
        if (next === d.width) return;
        d.width = next;
        // pointermove 可能比帧密：一帧只改一次宽度
        if (!d.frame) {
            d.frame = requestAnimationFrame(() => {
                const cur = drag.current;
                if (!cur) return;
                cur.frame = 0;
                onPreview(cur.width);
            });
        }
    };

    const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        const b = getBounds();
        const grow = side === 'left' ? 'ArrowRight' : 'ArrowLeft';
        const shrink = side === 'left' ? 'ArrowLeft' : 'ArrowRight';
        let next: number | null = null;
        if (e.key === grow) next = value + KEYBOARD_RESIZE_STEP;
        else if (e.key === shrink) next = value - KEYBOARD_RESIZE_STEP;
        else if (e.key === 'Home') next = b.min;
        else if (e.key === 'End') next = b.max;
        if (next === null) return;
        e.preventDefault();
        const w = clamp(next, b);
        if (w === value) return;
        onPreview(w);
        onCommit(w);
    };

    return (
        <div
            role="separator"
            aria-orientation="vertical"
            aria-label={label}
            aria-valuenow={value}
            aria-valuemin={bounds.min}
            aria-valuemax={bounds.max}
            tabIndex={0}
            title="拖动调整宽度，双击恢复默认"
            data-dragging={dragging || undefined}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={() => finish(true)}
            onPointerCancel={() => finish(false)}
            onLostPointerCapture={() => finish(true)}
            onDoubleClick={onReset}
            onKeyDown={onKeyDown}
            className={cn(
                'group relative z-10 w-1 shrink-0 cursor-col-resize touch-none select-none outline-none',
                // 看得见的只有中间 1px；能抓的范围只往右多伸 8px。左边紧挨着的是前一栏的滚动条，盖住就拖不了滚动条了
                "before:absolute before:inset-y-0 before:left-0 before:-right-2 before:content-['']",
            )}
        >
            <span
                aria-hidden
                className={cn(
                    'pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border-subtle transition-colors duration-150',
                    'group-hover:bg-brand/60 group-focus-visible:bg-brand group-data-[dragging]:bg-brand',
                )}
            />
            <span
                aria-hidden
                className={cn(
                    'pointer-events-none absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 rounded-pill bg-brand opacity-0 transition-opacity duration-150',
                    'group-focus-visible:opacity-100 group-data-[dragging]:opacity-100',
                )}
            />
        </div>
    );
}

function clamp(v: number, b: { min: number; max: number }): number {
    return Math.min(b.max, Math.max(b.min, Math.round(v)));
}

// 拖动期间整个窗口都是左右箭头光标、不选中文字：鼠标一快就会离开 4px 的条子，光标不该跟着闪回去
function startDragStyles(): void {
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
}

function endDragStyles(): void {
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
}

// ---------------------------------------------------------------------------
// 左栏收起后的窄边
// ---------------------------------------------------------------------------

export interface ColumnRailProps {
    active: DebugLeftPanel;
    /** 点某个面板的图标：切过去并展开 */
    onPick: (panel: DebugLeftPanel) => void;
    onExpand: () => void;
    width: number;
    appear?: boolean;
}

export function ColumnRail({ active, onPick, onExpand, width, appear = false }: ColumnRailProps) {
    const m = useMotion();
    const ref = useRef<HTMLElement>(null);
    const appearRef = useRef(appear);

    useLayoutEffect(() => {
        const el = ref.current;
        if (!el || !appearRef.current || !m.enabled || typeof el.animate !== 'function') return;
        const anim = el.animate([{ opacity: 0 }, { opacity: 1 }], {
            duration: m.duration('fast') * 1000,
            easing: cssEase(m.ease.enter),
        });
        return () => anim.cancel();
        // 只在挂载时播一次
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    return (
        <nav
            ref={ref}
            aria-label="左栏（已收起）"
            style={{ width }}
            className="flex shrink-0 flex-col items-center gap-1 border-r border-border-subtle py-1.5"
        >
            <RailButton label="展开左栏" onClick={onExpand} dataRailExpand>
                <ActionMotionIcon icon={PanelLeftOpen} size={15} strokeWidth={2} />
            </RailButton>
            <span aria-hidden className="my-1 h-px w-5 bg-border-subtle" />
            {LEFT_PANELS.map((p) => (
                <RailButton key={p.id} label={p.label} active={p.id === active} onClick={() => onPick(p.id)}>
                    <ActionMotionIcon icon={p.icon} size={15} strokeWidth={2} />
                </RailButton>
            ))}
        </nav>
    );
}

function RailButton({
    label,
    active = false,
    onClick,
    dataRailExpand = false,
    children,
}: {
    label: string;
    active?: boolean;
    onClick: () => void;
    /** 收起左栏后焦点落到这个按钮上（页面按这个标记找） */
    dataRailExpand?: boolean;
    children: ReactNode;
}) {
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    type="button"
                    aria-label={label}
                    aria-current={active ? 'true' : undefined}
                    data-rail-expand={dataRailExpand || undefined}
                    onClick={onClick}
                    className={cn(
                        'inline-flex h-8 w-8 items-center justify-center rounded-sm transition-colors',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        active
                            ? 'bg-brand-soft text-brand'
                            : 'text-text-tertiary hover:bg-inset hover:text-text',
                    )}
                >
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent side="right">{label}</TooltipContent>
        </Tooltip>
    );
}
