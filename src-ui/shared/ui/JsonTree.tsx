// JSON 树：把展开的节点压成一维行，再用虚拟列表只画看得见的那几十行。
// 调试台的响应、事件详情都用它看，几万行的响应也不卡。

import {
    useCallback,
    useEffect,
    useId,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent,
    type ReactNode,
    type Ref,
} from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Check, ChevronRight, Copy } from 'lucide-react';
import { cn } from '../utils/cn';
import { useMotion } from '../../hooks/preferences/useMotion';

export type JsonTreePath = Array<string | number>;

export interface JsonTreeNodeContext {
    /** 从根到该节点的路径：对象取键名（字符串），数组取下标（数字）。根节点是空数组。 */
    path: JsonTreePath;
    /** 路径最后一段的字符串形式；根节点没有。 */
    key?: string;
    value: unknown;
}

export interface JsonTreeHandle {
    expandAll: () => void;
    collapseAll: () => void;
}

export interface JsonTreeProps {
    value: unknown;
    /** 默认展开到第几层（根是第 0 层，小于该值的容器展开）。 */
    defaultExpandDepth?: number;
    /** 点某一行的值。容器行还会同时切换展开；键盘上对叶子行按 Enter 等价于点它的值。 */
    onValueClick?: (ctx: JsonTreeNodeContext) => void;
    /** 行尾悬停时和「复制」并排显示的额外操作。 */
    valueActions?: (ctx: JsonTreeNodeContext) => ReactNode;
    /** 宿主要有确定高度：flex 列里给 flex-1（默认已带），或直接给固定 h-*，否则虚拟列表会把全部行画出来。 */
    className?: string;
    handleRef?: Ref<JsonTreeHandle>;
}

const ROW_HEIGHT = 22;
const INDENT_PX = 14;
const LONG_STRING = 200;
// 展开后的长字符串照样只画有限的字数：几 MB 的 base64 塞进一行会让整页卡住
const LONG_STRING_RENDER_CAP = 20_000;
// 默认展开的总行数预算：数组元素很多时，只把前面一部分元素展开，剩下的收着
const AUTO_EXPAND_ROW_BUDGET = 20_000;
// 「全部展开」也要设上限，避免一次生成几百万行把界面冻住
const MAX_ROWS = 200_000;
// 防御性的深度上限：数据来自 JSON 不会有环，但传进来的毕竟是 unknown
const MAX_DEPTH = 100;

const ROOT_ID = '$';
// 路径串用不可见分隔符拼接，避免键名里带 `/` 之类时和别的路径撞名
const SEP = '\u0001';

type ContainerKind = 'object' | 'array';
type ExpandMode = 'auto' | 'all' | 'none';

interface Row {
    id: string;
    /** 父行在 rows 里的下标，根是 -1。路径按需顺着它往上拼，不给每行都存一份数组。 */
    parent: number;
    depth: number;
    /** 对象的键名（字符串）或数组下标（数字）；根为 null。 */
    pathKey: string | number | null;
    value: unknown;
    container: ContainerKind | null;
    expandable: boolean;
    expanded: boolean;
    /** 行数到上限后追加的一行提示，不对应任何节点。 */
    notice?: boolean;
}

function containerKind(v: unknown): ContainerKind | null {
    if (Array.isArray(v)) return 'array';
    if (v !== null && typeof v === 'object') return 'object';
    return null;
}

function childCountOf(v: unknown): number {
    if (Array.isArray(v)) return v.length;
    if (v !== null && typeof v === 'object') return Object.keys(v).length;
    return 0;
}

/**
 * 按默认深度算出「默认展开」的容器，并用总行数预算兜底。
 * 单独成一步、不看用户的手动开合：否则用户收起一个节点会腾出预算，
 * 让后面某个不相干的节点自己弹开。
 */
function computeAutoExpanded(root: unknown, maxDepth: number): Set<string> {
    const expanded = new Set<string>();
    if (maxDepth <= 0) return expanded;
    let budget = AUTO_EXPAND_ROW_BUDGET;
    const queue: Array<{ value: unknown; id: string; depth: number }> = [
        { value: root, id: ROOT_ID, depth: 0 },
    ];
    // 广度优先：浅层先用预算，深层用不起了就收着
    for (let head = 0; head < queue.length && budget > 0; head += 1) {
        const { value, id, depth } = queue[head]!;
        const kind = containerKind(value);
        if (!kind || depth >= maxDepth) continue;
        const n = childCountOf(value);
        if (n === 0) continue;
        // 根一定展开（否则什么都看不见），其余的预算不够就跳过
        if (depth > 0 && budget < n) continue;
        expanded.add(id);
        budget -= n;
        if (kind === 'array') {
            const arr = value as unknown[];
            for (let i = 0; i < arr.length; i += 1) {
                if (containerKind(arr[i]))
                    queue.push({ value: arr[i], id: id + SEP + i, depth: depth + 1 });
            }
        } else {
            const obj = value as Record<string, unknown>;
            for (const k of Object.keys(obj)) {
                if (containerKind(obj[k]))
                    queue.push({ value: obj[k], id: id + SEP + k, depth: depth + 1 });
            }
        }
    }
    return expanded;
}

/** 只沿展开的节点往下走，所以行数正比于「看得见的节点」，而不是整棵树。 */
function flatten(root: unknown, isExpanded: (id: string, depth: number) => boolean): Row[] {
    const rows: Row[] = [];
    let truncated = false;

    const walk = (
        value: unknown,
        id: string,
        depth: number,
        pathKey: string | number | null,
        parent: number,
    ) => {
        if (rows.length >= MAX_ROWS) {
            truncated = true;
            return;
        }
        const container = containerKind(value);
        const expandable = container !== null && childCountOf(value) > 0;
        const expanded = expandable && depth < MAX_DEPTH && isExpanded(id, depth);
        const index = rows.length;
        rows.push({ id, parent, depth, pathKey, value, container, expandable, expanded });
        if (!expanded) return;
        if (container === 'array') {
            const arr = value as unknown[];
            for (let i = 0; i < arr.length; i += 1) walk(arr[i], id + SEP + i, depth + 1, i, index);
        } else {
            const obj = value as Record<string, unknown>;
            for (const k of Object.keys(obj)) walk(obj[k], id + SEP + k, depth + 1, k, index);
        }
    };

    walk(root, ROOT_ID, 0, null, -1);
    if (truncated) {
        rows.push({
            id: `${ROOT_ID}${SEP}…notice`,
            parent: -1,
            depth: 0,
            pathKey: null,
            value: undefined,
            container: null,
            expandable: false,
            expanded: false,
            notice: true,
        });
    }
    return rows;
}

function pathOf(rows: Row[], index: number): JsonTreePath {
    const out: JsonTreePath = [];
    for (let i = index; i > 0; i = rows[i]!.parent) out.push(rows[i]!.pathKey as string | number);
    return out.reverse();
}

function contextOf(rows: Row[], index: number): JsonTreeNodeContext {
    const path = pathOf(rows, index);
    const last = path[path.length - 1];
    return { path, key: last === undefined ? undefined : String(last), value: rows[index]!.value };
}

/**
 * 找选中行在当前行集里的下标。选中行被折进祖先里（收起祖先、全部收起、换了新值）时，
 * 沿路径串一层层退到还看得见的最近祖先；根行永远在，所以只要有行就一定找得到。
 */
function resolveActiveIndex(rows: Row[], activeId: string | null): number {
    if (activeId === null) return -1;
    let candidate = activeId;
    for (;;) {
        const index = rows.findIndex((r) => r.id === candidate);
        if (index >= 0) return index;
        const cut = candidate.lastIndexOf(SEP);
        if (cut < 0) return -1;
        candidate = candidate.slice(0, cut);
    }
}

function toneClass(v: unknown): string {
    if (typeof v === 'string') return 'text-brand';
    if (typeof v === 'number' || typeof v === 'bigint') return 'text-info';
    if (typeof v === 'boolean') return 'text-warning';
    return 'text-text-tertiary';
}

function primitiveText(v: unknown): string {
    if (typeof v === 'string') return JSON.stringify(v);
    if (v === null) return 'null';
    if (v === undefined) return 'undefined';
    return String(v);
}

function copyText(v: unknown): string {
    try {
        return JSON.stringify(v, null, 2) ?? String(v);
    } catch {
        return String(v);
    }
}

const EMPTY_OVERRIDES: ReadonlyMap<string, boolean> = new Map();
const EMPTY_IDS: ReadonlySet<string> = new Set();

export function JsonTree({
    value,
    defaultExpandDepth = 2,
    onValueClick,
    valueActions,
    className,
    handleRef,
}: JsonTreeProps) {
    const parentRef = useRef<HTMLDivElement | null>(null);
    const baseId = useId();
    const m = useMotion();
    const animate = m.enabled;

    // 展开状态按路径串记：value 换成新对象（重新调用同一动作）后，同一路径仍保持用户展开的样子
    const [mode, setMode] = useState<ExpandMode>('auto');
    const [overrides, setOverrides] = useState<ReadonlyMap<string, boolean>>(EMPTY_OVERRIDES);
    const [openStrings, setOpenStrings] = useState<ReadonlySet<string>>(EMPTY_IDS);
    const [activeId, setActiveId] = useState<string | null>(null);
    // 「已复制」的反馈放在树上而不是每行里：行是虚拟的，滚出视口就卸载，键盘复制也没有行内按钮可显示
    const [copiedId, setCopiedId] = useState<string | null>(null);
    const copyTimerRef = useRef<number | null>(null);

    useEffect(
        () => () => {
            if (copyTimerRef.current !== null) window.clearTimeout(copyTimerRef.current);
        },
        [],
    );

    const copyNode = useCallback(async (id: string, node: unknown) => {
        try {
            await navigator.clipboard.writeText(copyText(node));
        } catch {
            // webview 没有 clipboard API 时静默失败，和别处的复制按钮一致
            return;
        }
        setCopiedId(id);
        if (copyTimerRef.current !== null) window.clearTimeout(copyTimerRef.current);
        copyTimerRef.current = window.setTimeout(() => setCopiedId(null), 1500);
    }, []);

    const autoExpanded = useMemo(
        () => computeAutoExpanded(value, defaultExpandDepth),
        [value, defaultExpandDepth],
    );
    const rows = useMemo(
        () =>
            flatten(value, (id, depth) => {
                const manual = overrides.get(id);
                if (manual !== undefined) return manual;
                if (mode === 'all') return true;
                if (mode === 'none') return depth === 0;
                return autoExpanded.has(id);
            }),
        [value, mode, overrides, autoExpanded],
    );

    const virtualizer = useVirtualizer({
        count: rows.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => ROW_HEIGHT,
        getItemKey: (index) => rows[index]?.id ?? index,
        overscan: 12,
    });

    useImperativeHandle(
        handleRef,
        () => ({
            expandAll: () => {
                setMode('all');
                setOverrides(EMPTY_OVERRIDES);
            },
            collapseAll: () => {
                setMode('none');
                setOverrides(EMPTY_OVERRIDES);
            },
        }),
        [],
    );

    const setExpanded = useCallback((id: string, next: boolean) => {
        setOverrides((prev) => new Map(prev).set(id, next));
    }, []);

    const toggleString = useCallback((id: string) => {
        setOpenStrings((prev) => {
            const next = new Set(prev);
            if (!next.delete(id)) next.add(id);
            return next;
        });
    }, []);

    // 行数可能几十万，只在行集或选中项变了才重找，不要每次滚动重渲都扫一遍
    const activeIndex = useMemo(() => resolveActiveIndex(rows, activeId), [rows, activeId]);

    // 选中行藏进折叠的祖先后，选中项也改落到那个祖先上，键盘从它接着走，而不是掉回第一行
    useEffect(() => {
        const resolved = rows[activeIndex];
        if (resolved && resolved.id !== activeId) setActiveId(resolved.id);
    }, [rows, activeIndex, activeId]);

    const moveTo = (index: number) => {
        const target = Math.min(Math.max(index, 0), rows.length - 1);
        const row = rows[target];
        if (!row) return;
        setActiveId(row.id);
        virtualizer.scrollToIndex(target, { align: 'auto' });
    };

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        // 焦点在行里的按钮上时，方向键、回车留给按钮自己
        if (e.target !== e.currentTarget || rows.length === 0) return;
        const current = activeIndex < 0 ? 0 : activeIndex;
        const row = rows[current]!;
        switch (e.key) {
            case 'ArrowDown':
                moveTo(current + 1);
                break;
            case 'ArrowUp':
                moveTo(current - 1);
                break;
            case 'Home':
                moveTo(0);
                break;
            case 'End':
                moveTo(rows.length - 1);
                break;
            case 'ArrowRight':
                if (row.expandable && !row.expanded) setExpanded(row.id, true);
                else if (row.expandable) moveTo(current + 1);
                break;
            case 'ArrowLeft':
                if (row.expandable && row.expanded) setExpanded(row.id, false);
                else if (row.parent >= 0) moveTo(row.parent);
                break;
            case 'c':
                // 带修饰键的留给浏览器（Ctrl+C 复制选中文字）
                if (e.ctrlKey || e.metaKey || e.altKey) return;
                if (!row.notice) void copyNode(row.id, row.value);
                break;
            case 'Enter':
            case ' ':
                if (row.notice) break;
                if (row.expandable) setExpanded(row.id, !row.expanded);
                else onValueClick?.(contextOf(rows, current));
                break;
            default:
                return;
        }
        e.preventDefault();
    };

    const items = virtualizer.getVirtualItems();

    return (
        <div
            ref={parentRef}
            role="tree"
            aria-label="JSON 树（方向键浏览，按 c 复制当前节点）"
            tabIndex={0}
            aria-activedescendant={activeIndex >= 0 ? `${baseId}-${activeIndex}` : undefined}
            onKeyDown={onKeyDown}
            onFocus={(e) => {
                // 键盘 Tab 进来时先落在第一行，方向键才有起点
                if (e.target === e.currentTarget && activeId === null && rows[0])
                    setActiveId(rows[0].id);
            }}
            className={cn(
                'min-h-0 flex-1 overflow-auto rounded-sm bg-inset font-mono text-xs outline-none',
                'focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-inset',
                className,
            )}
        >
            <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
                {items.map((virtualRow) => {
                    const row = rows[virtualRow.index];
                    if (!row) return null;
                    return (
                        <TreeRowView
                            key={row.id}
                            row={row}
                            index={virtualRow.index}
                            domId={`${baseId}-${virtualRow.index}`}
                            top={virtualRow.start}
                            active={row.id === activeId}
                            animate={animate}
                            stringOpen={openStrings.has(row.id)}
                            copied={copiedId === row.id}
                            onCopy={() => void copyNode(row.id, row.value)}
                            measureRef={virtualizer.measureElement}
                            context={() => contextOf(rows, virtualRow.index)}
                            renderActions={valueActions}
                            onValueClick={onValueClick}
                            onActivate={() => {
                                setActiveId(row.id);
                                if (row.expandable) setExpanded(row.id, !row.expanded);
                            }}
                            onToggleString={() => toggleString(row.id)}
                        />
                    );
                })}
            </div>
        </div>
    );
}

interface TreeRowViewProps {
    row: Row;
    index: number;
    domId: string;
    top: number;
    active: boolean;
    animate: boolean;
    stringOpen: boolean;
    copied: boolean;
    onCopy: () => void;
    measureRef: (node: Element | null) => void;
    /** 需要路径时才现算（点击、渲染行尾操作），平时不为每行拼数组。 */
    context: () => JsonTreeNodeContext;
    renderActions?: (ctx: JsonTreeNodeContext) => ReactNode;
    onValueClick?: (ctx: JsonTreeNodeContext) => void;
    onActivate: () => void;
    onToggleString: () => void;
}

function TreeRowView({
    row,
    index,
    domId,
    top,
    active,
    animate,
    stringOpen,
    copied,
    onCopy,
    measureRef,
    context,
    renderActions,
    onValueClick,
    onActivate,
    onToggleString,
}: TreeRowViewProps) {
    if (row.notice) {
        return (
            <div
                ref={measureRef}
                data-index={index}
                className="absolute left-0 top-0 w-full px-3 text-text-tertiary"
                style={{ transform: `translateY(${top}px)`, lineHeight: `${ROW_HEIGHT}px` }}
            >
                已到 {MAX_ROWS.toLocaleString()} 行上限，后面的节点没有画出来。请只展开需要的部分。
            </div>
        );
    }

    const keyLabel = row.pathKey === null ? null : String(row.pathKey);
    const keyIsIndex = typeof row.pathKey === 'number';
    const isString = typeof row.value === 'string';
    const longString = isString && (row.value as string).length > LONG_STRING;

    let valueNode: ReactNode;
    if (row.container) {
        const n = childCountOf(row.value);
        const open = row.container === 'array' ? '[' : '{';
        const close = row.container === 'array' ? ']' : '}';
        valueNode =
            n === 0 ? (
                <span className="text-text-tertiary">{open + close}</span>
            ) : (
                <span className="text-text-tertiary">
                    {`${open}…${close}`}
                    <span className="ml-1.5">{n} 项</span>
                </span>
            );
    } else if (longString && stringOpen) {
        const full = row.value as string;
        const shown =
            full.length > LONG_STRING_RENDER_CAP ? full.slice(0, LONG_STRING_RENDER_CAP) : full;
        valueNode = (
            <span className={cn(toneClass(row.value), 'min-w-0 whitespace-pre-wrap break-all')}>
                {`"${shown}"`}
                {full.length > LONG_STRING_RENDER_CAP && (
                    <span className="text-text-tertiary">
                        {` …（已截断，共 ${full.length.toLocaleString()} 字符，复制可得全文）`}
                    </span>
                )}
            </span>
        );
    } else if (longString) {
        const head = JSON.stringify((row.value as string).slice(0, LONG_STRING)).slice(0, -1);
        valueNode = (
            <span className={cn(toneClass(row.value), 'min-w-0 truncate')}>{`${head}…"`}</span>
        );
    } else {
        valueNode = (
            <span className={cn(toneClass(row.value), 'min-w-0 truncate')}>
                {primitiveText(row.value)}
            </span>
        );
    }

    const handleValueClick = onValueClick
        ? () => {
              onValueClick(context());
          }
        : undefined;

    return (
        <div
            ref={measureRef}
            id={domId}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={row.expandable ? row.expanded : undefined}
            aria-selected={active}
            data-index={index}
            onClick={onActivate}
            className={cn(
                'group absolute left-0 top-0 flex w-full items-start gap-1 pr-2',
                active ? 'bg-brand-soft' : 'hover:bg-elevated',
            )}
            style={{
                transform: `translateY(${top}px)`,
                paddingLeft: 8 + row.depth * INDENT_PX,
                minHeight: ROW_HEIGHT,
                lineHeight: `${ROW_HEIGHT}px`,
            }}
        >
            <span
                className="flex shrink-0 items-center"
                style={{ height: ROW_HEIGHT, width: 12 }}
                aria-hidden
            >
                {row.expandable && (
                    <ChevronRight
                        size={12}
                        className={cn(
                            'text-text-tertiary',
                            // 只动 transform，且尊重「减少动效」
                            animate && 'transition-transform duration-150',
                            row.expanded && 'rotate-90',
                        )}
                    />
                )}
            </span>
            {keyLabel !== null && (
                <>
                    <span
                        className={cn(
                            'shrink-0',
                            keyIsIndex ? 'text-text-tertiary' : 'text-text-secondary',
                        )}
                    >
                        {keyLabel}
                    </span>
                    <span className="shrink-0 text-text-tertiary">:</span>
                </>
            )}
            <span
                className={cn(
                    'flex min-w-0 items-start gap-2',
                    handleValueClick && 'cursor-pointer hover:underline',
                )}
                onClick={handleValueClick}
            >
                {valueNode}
            </span>
            {longString && (
                <button
                    type="button"
                    // 行里的按钮不进 Tab 顺序：否则 Tab 要按几十下才出得了树，方向键也会因焦点落在按钮上失效
                    tabIndex={-1}
                    className="shrink-0 text-brand hover:underline"
                    onClick={(e) => {
                        e.stopPropagation();
                        onToggleString();
                    }}
                >
                    {stringOpen ? '收起' : '展开'}
                </button>
            )}
            <span
                className={cn(
                    'ml-auto flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100',
                    active && 'opacity-100',
                )}
                style={{ height: ROW_HEIGHT }}
                onClick={(e) => e.stopPropagation()}
            >
                <CopyNodeButton copied={copied} onCopy={onCopy} />
                {renderActions?.(context())}
            </span>
        </div>
    );
}

function CopyNodeButton({ copied, onCopy }: { copied: boolean; onCopy: () => void }) {
    return (
        <button
            type="button"
            // 不进 Tab 顺序，键盘用户在树上按 c 复制当前节点
            tabIndex={-1}
            title={copied ? '已复制' : '复制'}
            aria-label={copied ? '已复制' : '复制该节点的 JSON'}
            className="flex h-[18px] w-[18px] items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-text"
            onClick={onCopy}
        >
            {copied ? <Check size={12} /> : <Copy size={12} />}
        </button>
    );
}
