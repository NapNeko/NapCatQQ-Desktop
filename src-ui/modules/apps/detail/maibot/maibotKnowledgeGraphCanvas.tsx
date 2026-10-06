// 记忆图谱的画布：SVG，拖空白处平移、滚轮以指针为中心缩放、双击空白处放回全图。
// 点和边在一层 transform 里，平移只改这一层的 transform，不重画几百个元素；
// 字在屏幕坐标里单独一层，缩放时不跟着变大变小。悬停或选中一个点时，只亮它和它的邻居。

import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { boundsOf, fitView, type Point } from '../../../../core/domain/apps/graphLayout';
import type { MaiBotMemoryGraph } from '../../../../core/ipc/types';
import { cn } from '../../../../shared/utils/cn';

export type GraphView = { x: number; y: number; k: number };

const radiusOf = (degree: number) => Math.min(16, 4 + Math.sqrt(degree) * 2.2);
// 字描一圈底色的边，压在线上也看得清
const LABEL = '[paint-order:stroke] stroke-field [stroke-width:3px] [stroke-linejoin:round]';

const Shapes = memo(function Shapes({
    graph,
    layout,
    focus,
    near,
    k,
    selected,
    onHover,
    onSelect,
}: {
    graph: MaiBotMemoryGraph;
    layout: Map<string, Point>;
    focus: string | null;
    near: ReadonlySet<string>;
    k: number;
    selected: string | null;
    onHover: (id: string | null) => void;
    onSelect: (id: string) => void;
}) {
    return (
        <>
            {graph.edges.map((e) => {
                const a = layout.get(e.source);
                const b = layout.get(e.target);
                if (!a || !b) return null;
                const lit = focus !== null && (e.source === focus || e.target === focus);
                return (
                    <line
                        key={`${e.source}\u0000${e.target}`}
                        x1={a.x}
                        y1={a.y}
                        x2={b.x}
                        y2={b.y}
                        vectorEffect="non-scaling-stroke"
                        strokeWidth={lit ? 1.6 : Math.min(2.4, 0.8 + e.relations * 0.3)}
                        className={cn(
                            'transition-[stroke-opacity] duration-200',
                            lit ? 'stroke-brand' : 'stroke-text-tertiary',
                        )}
                        strokeOpacity={focus === null ? 0.35 : lit ? 0.9 : 0.08}
                    />
                );
            })}
            {graph.nodes.map((n) => {
                const p = layout.get(n.id);
                if (!p) return null;
                const dim = focus !== null && !near.has(n.id);
                const r = radiusOf(n.degree) / k;
                return (
                    <g key={n.id}>
                        {/* 实心垫底：上面那层是半透明的柔色，不垫的话线会从点里透出来 */}
                        <circle
                            cx={p.x}
                            cy={p.y}
                            r={r}
                            className="pointer-events-none fill-field"
                        />
                        <circle
                            cx={p.x}
                            cy={p.y}
                            r={r}
                            onPointerEnter={() => onHover(n.id)}
                            onPointerLeave={() => onHover(null)}
                            onClick={(ev) => {
                                ev.stopPropagation();
                                onSelect(n.id);
                            }}
                            className={cn(
                                'cursor-pointer transition-[fill-opacity,stroke-width] duration-200',
                                n.id === selected
                                    ? 'fill-brand stroke-surface'
                                    : near.has(n.id) && focus !== null
                                      ? 'fill-brand/70 stroke-surface'
                                      : 'fill-info/70 stroke-surface',
                            )}
                            strokeWidth={(n.id === selected ? 3 : 1.5) / k}
                            fillOpacity={dim ? 0.2 : 1}
                        />
                    </g>
                );
            })}
        </>
    );
});

export const GraphCanvas: React.FC<{
    graph: MaiBotMemoryGraph;
    layout: Map<string, Point>;
    selected: string | null;
    onSelect: (id: string | null) => void;
    /** 递增一次就把视图放回全图 */
    fitSignal: number;
    /** 想让某个点居中时给它（搜到后跳过去） */
    centerOn: string | null;
}> = ({ graph, layout, selected, onSelect, fitSignal, centerOn }) => {
    const box = useRef<HTMLDivElement>(null);
    const [size, setSize] = useState({ w: 0, h: 0 });
    const [view, setView] = useState<GraphView>({ x: 0, y: 0, k: 1 });
    const [hover, setHover] = useState<string | null>(null);
    const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);

    useLayoutEffect(() => {
        const el = box.current;
        if (!el) return;
        const ro = new ResizeObserver(([entry]) =>
            setSize({ w: entry.contentRect.width, h: entry.contentRect.height }),
        );
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    // 换了数据、第一次量到尺寸、点了「适应窗口」：放回全图
    const bounds = useMemo(() => boundsOf(layout.values()), [layout]);
    useEffect(() => {
        if (size.w > 0 && size.h > 0) setView(fitView(bounds, size.w, size.h));
    }, [bounds, size.w > 0 && size.h > 0, fitSignal]); // eslint-disable-line react-hooks/exhaustive-deps

    useEffect(() => {
        const p = centerOn ? layout.get(centerOn) : undefined;
        if (!p || size.w === 0) return;
        setView((v) => {
            const k = Math.max(v.k, 1.1);
            return { k, x: size.w / 2 - p.x * k, y: size.h / 2 - p.y * k };
        });
    }, [centerOn, layout, size.w, size.h]);

    const focus = hover ?? selected;
    const near = useMemo(() => {
        const s = new Set<string>();
        if (!focus) return s;
        s.add(focus);
        for (const e of graph.edges) {
            if (e.source === focus) s.add(e.target);
            else if (e.target === focus) s.add(e.source);
        }
        return s;
    }, [focus, graph.edges]);

    // 有名字的：度数排前两成的、放大以后的、正亮着的
    const labelled = useMemo(() => {
        const sorted = [...graph.nodes].sort((a, b) => b.degree - a.degree);
        const cut = sorted[Math.max(0, Math.floor(sorted.length * 0.2) - 1)]?.degree ?? 0;
        return new Set(sorted.filter((n) => n.degree >= Math.max(2, cut)).map((n) => n.id));
    }, [graph.nodes]);
    const toScreen = (p: Point) => ({ x: p.x * view.k + view.x, y: p.y * view.k + view.y });

    const onWheel = (ev: React.WheelEvent) => {
        const rect = box.current?.getBoundingClientRect();
        if (!rect) return;
        const px = ev.clientX - rect.left;
        const py = ev.clientY - rect.top;
        setView((v) => {
            const k = Math.min(4, Math.max(0.15, v.k * Math.exp(-ev.deltaY * 0.0015)));
            return { k, x: px - (px - v.x) * (k / v.k), y: py - (py - v.y) * (k / v.k) };
        });
    };

    return (
        <div
            ref={box}
            className="relative h-full w-full overflow-hidden rounded-md border border-border-subtle bg-field"
        >
            <svg
                width={size.w}
                height={size.h}
                className="block cursor-grab touch-none select-none active:cursor-grabbing"
                onWheel={onWheel}
                onPointerDown={(ev) => {
                    drag.current = { x: ev.clientX, y: ev.clientY, moved: false };
                }}
                onPointerMove={(ev) => {
                    const d = drag.current;
                    if (!d) return;
                    const dx = ev.clientX - d.x;
                    const dy = ev.clientY - d.y;
                    if (!d.moved && Math.hypot(dx, dy) < 3) return;
                    // 真拖起来才抓指针：一按下就抓的话，点在圆点上的 click 会被改派给 svg，点不中节点
                    if (!d.moved) (ev.currentTarget as Element).setPointerCapture(ev.pointerId);
                    d.moved = true;
                    d.x = ev.clientX;
                    d.y = ev.clientY;
                    setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
                }}
                onPointerUp={(ev) => {
                    const d = drag.current;
                    drag.current = null;
                    // 在空白处点一下（没拖动）：取消选中
                    if (d && !d.moved && ev.target === ev.currentTarget) onSelect(null);
                }}
                onDoubleClick={(ev) =>
                    ev.target === ev.currentTarget && setView(fitView(bounds, size.w, size.h))
                }
            >
                <g transform={`translate(${view.x},${view.y}) scale(${view.k})`}>
                    <Shapes
                        graph={graph}
                        layout={layout}
                        focus={focus}
                        near={near}
                        k={view.k}
                        selected={selected}
                        onHover={setHover}
                        onSelect={onSelect}
                    />
                </g>
                <g className="pointer-events-none">
                    {focus &&
                        graph.edges
                            .filter((e) => (e.source === focus || e.target === focus) && e.label)
                            .map((e) => {
                                const a = layout.get(e.source);
                                const b = layout.get(e.target);
                                if (!a || !b) return null;
                                const m = toScreen({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
                                return (
                                    <text
                                        key={`l:${e.source}\u0000${e.target}`}
                                        x={m.x}
                                        y={m.y}
                                        textAnchor="middle"
                                        className={cn(LABEL, 'fill-brand text-[10px]')}
                                    >
                                        {e.label}
                                    </text>
                                );
                            })}
                    {graph.nodes.map((n) => {
                        const show = near.has(n.id) || labelled.has(n.id) || view.k >= 1.5;
                        const p = layout.get(n.id);
                        if (!show || !p) return null;
                        const s = toScreen(p);
                        const dim = focus !== null && !near.has(n.id);
                        return (
                            <text
                                key={`n:${n.id}`}
                                x={s.x + radiusOf(n.degree) + 3}
                                y={s.y + 4}
                                className={cn(
                                    LABEL,
                                    'text-[11px]',
                                    n.id === focus
                                        ? 'fill-text font-semibold'
                                        : 'fill-text-secondary',
                                )}
                                opacity={dim ? 0.25 : 1}
                            >
                                {n.id}
                            </text>
                        );
                    })}
                </g>
            </svg>
            <p className="pointer-events-none absolute bottom-2 left-3 text-2xs text-text-disabled">
                拖动平移 · 滚轮缩放 · 双击空白处看全图
            </p>
        </div>
    );
};
