// 数据表：调试台里群列表、好友列表这类「一组同形对象」的响应用它看。
// 表头吸顶、点击排序、全文过滤，行走虚拟列表，几万行也只画看得见的部分。

import { useDeferredValue, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDown, ArrowUp, Search } from 'lucide-react';
import { cn } from '../utils/cn';

export interface DataTableCellContext {
    row: Record<string, unknown>;
    column: string;
    value: unknown;
}

export interface DataTableProps {
    columns: string[];
    rows: Array<Record<string, unknown>>;
    onCellClick?: (ctx: DataTableCellContext) => void;
    /** 宿主要有确定高度：flex 列里给 flex-1（默认已带），或直接给固定 h-*，否则虚拟列表会把全部行画出来。 */
    className?: string;
}

const ROW_HEIGHT = 28;
const HEADER_HEIGHT = 28;
const COLUMN_MIN_WIDTH = 120;
// 单元格只画前面这么多字：一格里塞几万字符的字符串没意义，CSS 也早把它截成一行了
const CELL_RENDER_CAP = 300;
const CELL_TITLE_CAP = 1000;

type SortDir = 'asc' | 'desc';
interface SortState {
    column: string;
    dir: SortDir;
}

/** 单元格显示文本：对象、数组压成单行 JSON，null 显式写出来，undefined 留空。 */
function cellText(v: unknown): string {
    if (typeof v === 'string') return v;
    if (v === undefined) return '';
    if (v === null) return 'null';
    if (typeof v === 'object') {
        try {
            return JSON.stringify(v) ?? String(v);
        } catch {
            return String(v);
        }
    }
    return String(v);
}

const NUMERIC_STRING = /^-?\d+(?:\.\d+)?$/;

function numericValue(v: unknown): number | null {
    if (typeof v === 'number') return Number.isNaN(v) ? null : v;
    if (typeof v === 'string' && NUMERIC_STRING.test(v.trim())) return Number(v);
    return null;
}

// numeric: 让 "群 2" 排在 "群 10" 前面
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

interface SortKey {
    index: number;
    empty: boolean;
    num: number | null;
    text: string;
}

/**
 * 对一组行下标按某列排序。两边都能当数字就按数值比（群号、时间戳存成字符串也一样），
 * 否则按自然序比文本；空值不论升降都排最后。先把键算好再排，避免比较函数里反复转换。
 */
function sortIndices(rows: Array<Record<string, unknown>>, indices: number[], sort: SortState): number[] {
    const sign = sort.dir === 'asc' ? 1 : -1;
    const keys: SortKey[] = indices.map((index) => {
        const v = rows[index]?.[sort.column];
        const empty = v === null || v === undefined || v === '';
        return { index, empty, num: empty ? null : numericValue(v), text: empty ? '' : cellText(v) };
    });
    keys.sort((a, b) => {
        if (a.empty || b.empty) return a.empty === b.empty ? 0 : a.empty ? 1 : -1;
        const cmp = a.num !== null && b.num !== null ? a.num - b.num : collator.compare(a.text, b.text);
        return cmp * sign;
    });
    return keys.map((k) => k.index);
}

export function DataTable({ columns, rows, onCellClick, className }: DataTableProps) {
    const scrollRef = useRef<HTMLDivElement | null>(null);
    const [sort, setSort] = useState<SortState | null>(null);
    const [filter, setFilter] = useState('');
    // 输入框吃即时的 filter，几万行的过滤和重排吃延后的值：敲字优先响应，表格随后跟上
    const deferredFilter = useDeferredValue(filter);
    const filterPending = filter !== deferredFilter;

    // 每行所有格子的小写文本拼成一串，过滤时只做一次 includes。
    // 首次真正过滤时才算，之后随 rows / columns 缓存，省得没用过滤的表白白多花时间。
    const haystack = useMemo(() => {
        let cache: string[] | null = null;
        return () => {
            cache ??= rows.map((row) => columns.map((c) => cellText(row[c])).join('\u0000').toLowerCase());
            return cache;
        };
    }, [rows, columns]);

    // 列被换掉后，指向已不存在列的排序直接忽略
    const activeSort = sort && columns.includes(sort.column) ? sort : null;

    const visible = useMemo(() => {
        let indices = rows.map((_, i) => i);
        const q = deferredFilter.trim().toLowerCase();
        if (q) {
            const hay = haystack();
            indices = indices.filter((i) => hay[i]!.includes(q));
        }
        return activeSort ? sortIndices(rows, indices, activeSort) : indices;
    }, [rows, deferredFilter, activeSort, haystack]);

    const virtualizer = useVirtualizer({
        count: visible.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => ROW_HEIGHT,
        getItemKey: (i) => visible[i] ?? i,
        overscan: 10,
        // 表头在滚动区里吸顶占了一截，行的起点要让出这段（getTotalSize 已经扣掉了），否则头几行会被表头盖住
        scrollMargin: HEADER_HEIGHT,
    });

    const cycleSort = (column: string) => {
        // 升序 → 降序 → 还原；换列从升序开始
        setSort((prev) => {
            if (!prev || prev.column !== column) return { column, dir: 'asc' };
            return prev.dir === 'asc' ? { column, dir: 'desc' } : null;
        });
    };

    const gridTemplateColumns = `repeat(${Math.max(columns.length, 1)}, minmax(${COLUMN_MIN_WIDTH}px, 1fr))`;
    const items = virtualizer.getVirtualItems();
    const filtered = visible.length !== rows.length;

    return (
        <div
            className={cn(
                'flex min-h-0 flex-1 flex-col overflow-hidden rounded-sm border border-border-subtle bg-inset',
                className,
            )}
        >
            <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle bg-surface px-2 py-1.5">
                <label className="relative flex min-w-0 max-w-64 flex-1 items-center">
                    <Search size={12} className="pointer-events-none absolute left-2 text-text-tertiary" aria-hidden />
                    <input
                        type="search"
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        placeholder="过滤所有列…"
                        aria-label="过滤表格行"
                        className={cn(
                            'h-6 w-full rounded-xs border border-border-subtle bg-field pl-6 pr-2 text-xs text-text',
                            'outline-none placeholder:text-text-tertiary',
                            'focus:border-brand focus:ring-1 focus:ring-brand focus:ring-inset',
                        )}
                    />
                </label>
                <span className="ml-auto shrink-0 text-2xs text-text-tertiary">
                    {filtered ? `${visible.length} / ${rows.length} 行` : `${rows.length} 行`}
                </span>
            </div>

            <div
                ref={scrollRef}
                role="table"
                aria-rowcount={visible.length + 1}
                aria-colcount={columns.length}
                aria-busy={filterPending}
                className={cn('min-h-0 flex-1 overflow-auto', filterPending && 'opacity-70')}
            >
                <div style={{ minWidth: Math.max(columns.length, 1) * COLUMN_MIN_WIDTH }}>
                    <div
                        role="row"
                        aria-rowindex={1}
                        className="sticky top-0 z-10 grid border-b border-border-subtle bg-surface"
                        style={{ gridTemplateColumns, height: HEADER_HEIGHT }}
                    >
                        {columns.map((column, ci) => {
                            const dir = activeSort?.column === column ? activeSort.dir : null;
                            return (
                                <div
                                    key={column}
                                    role="columnheader"
                                    aria-colindex={ci + 1}
                                    aria-sort={dir === 'asc' ? 'ascending' : dir === 'desc' ? 'descending' : 'none'}
                                    className="min-w-0"
                                >
                                    <button
                                        type="button"
                                        onClick={() => cycleSort(column)}
                                        title={column}
                                        className={cn(
                                            'flex h-full w-full items-center gap-1 px-2 text-left text-xs font-medium',
                                            'hover:bg-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-inset',
                                            dir ? 'text-text' : 'text-text-secondary',
                                        )}
                                    >
                                        <span className="min-w-0 truncate">{column}</span>
                                        {dir === 'asc' && <ArrowUp size={12} className="shrink-0 text-brand" aria-hidden />}
                                        {dir === 'desc' && <ArrowDown size={12} className="shrink-0 text-brand" aria-hidden />}
                                    </button>
                                </div>
                            );
                        })}
                    </div>

                    {visible.length === 0 ? (
                        <div className="py-8 text-center text-xs text-text-tertiary">
                            {rows.length === 0 ? '没有数据' : '没有匹配的行'}
                        </div>
                    ) : (
                        <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
                            {items.map((virtualRow) => {
                                const rowIndex = visible[virtualRow.index];
                                const row = rowIndex === undefined ? undefined : rows[rowIndex];
                                if (!row) return null;
                                return (
                                    <div
                                        key={virtualRow.key}
                                        role="row"
                                        aria-rowindex={virtualRow.index + 2}
                                        className="absolute left-0 top-0 grid w-full border-b border-border-subtle/60 hover:bg-elevated"
                                        style={{
                                            gridTemplateColumns,
                                            height: ROW_HEIGHT,
                                            transform: `translateY(${virtualRow.start - virtualizer.options.scrollMargin}px)`,
                                        }}
                                    >
                                        {columns.map((column, ci) => {
                                            const value = row[column];
                                            const text = cellText(value);
                                            return (
                                                <div
                                                    key={column}
                                                    role="cell"
                                                    aria-colindex={ci + 1}
                                                    title={text.length > CELL_TITLE_CAP ? text.slice(0, CELL_TITLE_CAP) : text}
                                                    onClick={onCellClick ? () => onCellClick({ row, column, value }) : undefined}
                                                    // 可点的格子要能用键盘到达并触发；不可点的不占 Tab 位
                                                    tabIndex={onCellClick ? 0 : undefined}
                                                    onKeyDown={
                                                        onCellClick
                                                            ? (e) => {
                                                                if (e.key !== 'Enter') return;
                                                                e.preventDefault();
                                                                onCellClick({ row, column, value });
                                                            }
                                                            : undefined
                                                    }
                                                    className={cn(
                                                        'min-w-0 truncate px-2 font-mono text-xs',
                                                        value === null || value === undefined ? 'text-text-tertiary' : 'text-text',
                                                        onCellClick &&
                                                        'cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-inset',
                                                    )}
                                                    style={{ lineHeight: `${ROW_HEIGHT}px` }}
                                                >
                                                    {text.length > CELL_RENDER_CAP ? text.slice(0, CELL_RENDER_CAP) : text}
                                                </div>
                                            );
                                        })}
                                    </div>
                                );
                            })}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
