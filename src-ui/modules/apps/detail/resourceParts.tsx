// 资源页（表情包、学到的表达 / 黑话、人物…）共用的骨架：工具条固定在上、列表区自己滚、翻页贴底，
// 勾选后底部浮出批量条。这些页改了就落库，不挂保存条。

import { forwardRef, useEffect, useRef, useState, type ReactNode } from 'react';
import gsap from 'gsap';
import { ChevronLeft, ChevronRight, Search, X } from 'lucide-react';
import { Button, Checkbox } from '../../../shared/ui';
import { GsapPresence, type EnterFn, type ExitFn } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';

/** 搜索框输入停下来再查，别一个字一个请求 */
export function useDebounced<T>(value: T, ms = 300): T {
    const [settled, setSettled] = useState(value);
    useEffect(() => {
        const id = setTimeout(() => setSettled(value), ms);
        return () => clearTimeout(id);
    }, [value, ms]);
    return settled;
}

export const ResourcePane: React.FC<{
    toolbar: ReactNode;
    /** 工具条下面一行说明（「只有精选的才会用在回复里」这种） */
    notice?: ReactNode;
    footer?: ReactNode;
    /** 底部浮条（多选后出现），浮在列表上 */
    overlay?: ReactNode;
    /** 内容区自己铺满、自己滚（图谱画布这种）：不留给浮条的底边距 */
    fill?: boolean;
    children: ReactNode;
}> = ({ toolbar, notice, footer, overlay, fill, children }) => (
    <div className="relative flex min-h-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-2 pb-3">{toolbar}</div>
        {notice && <div className="pb-3">{notice}</div>}
        <div className={cn('min-h-0 flex-1', fill ? 'overflow-hidden' : 'overflow-y-auto pb-16')}>{children}</div>
        {footer && <div className="border-t border-border-subtle pt-2.5">{footer}</div>}
        {overlay}
    </div>
);

export const SearchBox: React.FC<{
    value: string;
    onChange: (next: string) => void;
    placeholder?: string;
    className?: string;
}> = ({ value, onChange, placeholder = '搜索', className }) => (
    <div className={cn('relative min-w-0', className)}>
        <Search size={13} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary" />
        <input
            type="search"
            aria-label={placeholder}
            placeholder={placeholder}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            className="h-8 w-full rounded-sm border border-transparent bg-inset/70 pl-8 pr-2 text-[12.5px] text-text outline-none transition-colors placeholder:text-text-tertiary focus:border-brand/50 focus:bg-surface"
        />
    </div>
);

export type SegmentItem<V extends string> = { value: V; label: string; count?: number };

/** 几档互斥的筛选，带条数。比下拉少点一下，一眼看得到各档多少 */
export function Segmented<V extends string>({
    items,
    value,
    onChange,
}: {
    items: readonly SegmentItem<V>[];
    value: V;
    onChange: (next: V) => void;
}) {
    return (
        <div role="radiogroup" className="inline-flex shrink-0 items-center gap-0.5 rounded-sm bg-inset/70 p-0.5">
            {items.map((it) => {
                const on = it.value === value;
                return (
                    <button
                        key={it.value}
                        type="button"
                        role="radio"
                        aria-checked={on}
                        onClick={() => onChange(it.value)}
                        className={cn(
                            'inline-flex h-7 items-center gap-1.5 rounded-[6px] px-2.5 text-[12.5px] transition-colors',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                            on ? 'bg-surface font-medium text-text shadow-sm' : 'text-text-secondary hover:text-text',
                        )}
                    >
                        {it.label}
                        {it.count !== undefined && (
                            <span className={cn('font-mono text-2xs', on ? 'text-text-tertiary' : 'text-text-disabled')}>
                                {it.count}
                            </span>
                        )}
                    </button>
                );
            })}
        </div>
    );
}

export const Pager: React.FC<{
    page: number;
    pageSize: number;
    total: number;
    onPage: (page: number) => void;
}> = ({ page, pageSize, total, onPage }) => {
    const pages = Math.max(1, Math.ceil(total / pageSize));
    return (
        <div className="flex items-center justify-between gap-3 text-xs text-text-tertiary">
            <span>共 {total} 条</span>
            {pages > 1 && (
                <div className="flex items-center gap-1">
                    <Button size="sm" variant="ghost" className="h-7 w-7 p-0" aria-label="上一页" disabled={page <= 1} onClick={() => onPage(page - 1)}>
                        <ChevronLeft size={14} />
                    </Button>
                    <span className="min-w-[4.5rem] text-center tabular-nums text-text-secondary">
                        {page} / {pages}
                    </span>
                    <Button size="sm" variant="ghost" className="h-7 w-7 p-0" aria-label="下一页" disabled={page >= pages} onClick={() => onPage(page + 1)}>
                        <ChevronRight size={14} />
                    </Button>
                </div>
            )}
        </div>
    );
};

const enter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, y: 16, scale: 0.97 },
        { autoAlpha: 1, y: 0, scale: 1, duration: env.duration('base'), ease: env.ease.release },
    );
const exit: ExitFn = (el, env) =>
    gsap.to(el, { autoAlpha: 0, y: 16, scale: 0.97, duration: env.duration('fast'), ease: env.ease.exit });

type SelectionProps = {
    count: number;
    onClear: () => void;
    /** 这一页还有没勾上的才给 */
    onSelectAll?: () => void;
    children: ReactNode;
};

const SelectionBody = forwardRef<HTMLDivElement, SelectionProps>(
    ({ count, onClear, onSelectAll, children }, ref) => (
        <div
            ref={ref}
            role="toolbar"
            aria-label="批量操作"
            className="pointer-events-auto flex items-center gap-1.5 whitespace-nowrap rounded-full bg-elevated py-1.5 pl-4 pr-1.5 shadow-popover ring-1 ring-border-subtle"
        >
            <span className="text-xs text-text-secondary">
                已选 <span className="font-semibold tabular-nums text-text">{count}</span> 条
            </span>
            {onSelectAll && (
                <button
                    type="button"
                    onClick={onSelectAll}
                    className="rounded-sm px-1 text-xs text-brand hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                >
                    全选本页
                </button>
            )}
            <span aria-hidden className="mx-1 h-4 w-px bg-border-subtle" />
            {children}
            <Button size="sm" variant="ghost" className="h-7 w-7 rounded-full p-0" aria-label="取消选择" onClick={onClear}>
                <X size={14} />
            </Button>
        </div>
    ),
);
SelectionBody.displayName = 'SelectionBody';

/** 勾选后浮在列表底部中间的批量条；外层只管居中，动效打在里层 */
export const SelectionBar: React.FC<SelectionProps> = ({ count, ...rest }) => {
    // 退场那几帧 count 已经归零，条上别闪成「已选 0 条」
    const shown = useRef(count);
    if (count > 0) shown.current = count;
    return (
        <div className="pointer-events-none absolute inset-x-0 bottom-14 flex justify-center">
            <GsapPresence visible={count > 0} onEnter={enter} onExit={exit}>
                <SelectionBody count={shown.current} {...rest} />
            </GsapPresence>
        </div>
    );
};

/**
 * 点一下勾选框要动哪几条。Shift 且上一下点的还在这页上：从那条到这条整段；否则只动这一条。
 * 整段跟着点的这条走：它原来没选就整段选上，选了就整段取消
 */
export function pickSpan<K>(picked: ReadonlySet<K>, anchor: K | null, k: K, shift: boolean, order: readonly K[]) {
    const from = shift && anchor !== null ? order.indexOf(anchor) : -1;
    const to = order.indexOf(k);
    const keys = from < 0 || to < 0 ? [k] : order.slice(Math.min(from, to), Math.max(from, to) + 1);
    return { keys, on: !picked.has(k) };
}

/** 多选的一套状态：换页、换筛选时由调用方清掉 */
export function useSelection<K>() {
    const [picked, setPicked] = useState<ReadonlySet<K>>(() => new Set());
    const anchor = useRef<K | null>(null);
    const setAll = (keys: readonly K[], on: boolean) =>
        setPicked((prev) => {
            const next = new Set(prev);
            for (const k of keys) {
                if (on) next.add(k);
                else next.delete(k);
            }
            return next;
        });
    return {
        picked,
        has: (k: K) => picked.has(k),
        /** order 是这一页列表的顺序，Shift 连选按它取区间 */
        pick: (k: K, shift: boolean, order: readonly K[]) => {
            const { keys, on } = pickSpan(picked, anchor.current, k, shift, order);
            anchor.current = k;
            setAll(keys, on);
        },
        setAll,
        clear: () => {
            anchor.current = null;
            setPicked(new Set());
        },
    };
}

/** 行首的勾选框：Shift 点连选；按下时不让浏览器顺手把中间几行的字也选蓝 */
export const RowCheck: React.FC<{ checked: boolean; onPick: (shift: boolean) => void }> = ({ checked, onPick }) => (
    <Checkbox
        aria-label="选中这一条"
        checked={checked}
        onMouseDown={(e) => e.shiftKey && e.preventDefault()}
        onClick={(e) => onPick(e.shiftKey)}
    />
);
