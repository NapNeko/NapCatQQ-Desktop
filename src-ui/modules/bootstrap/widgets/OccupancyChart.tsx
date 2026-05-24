// CPU / RAM 占用率折线图。
//
// 严格 1:1 对应 legacy `OccupancyPanel._OccupancyCanvas`
// (legacy-python/src/ui/page/home_page/widget/occupancy_card.py L75-L390)。
//
// 算法核心：
//   稳态：history N 个点（24）。曲线占满 [left, right]，最后一段触底 right。
//   tick 到来：保留上一稳态的 N 点为 valuesPrev，连同 history 最新点构成 N+1 点。
//   动画进行：progress 0 → 1 (linear, ~tick 时长)，每帧重画。
//     每个点 x = i * step_x - progress * step_x   (step_x = innerW / (N - 1))
//   两端裁切：用 clipDisplayPoints 在 x=0 / x=innerW 处线性插值，
//     曲线永远填满 [0, innerW]。
//   动画结束：丢弃最左旧点，valuesPrev 切到 history。
//
// hover 指示器（对应 legacy `_draw_hover_indicator`）：
//   - 鼠标进绘图区显示垂直引导线 + 穿过最近点的双层圆点
//   - 顶部显示 brand 色 pill 文字（百分比）
//   - hover 期间卡片右上角 valueText 切换到 hover 值，离开后切回当前 prop 值
//
// 静态层（Y 轴 + 网格）用普通 div，不被任何 transform 影响。

import React, { useEffect, useMemo, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import { Card } from '../../../shared/ui';
import type { ResourcePoint } from '../../../hooks/diagnostics/useResourceMonitor';

interface OccupancyChartProps {
    title: string;
    icon: LucideIcon;
    history: ResourcePoint[];
    dataKey: 'cpu' | 'ram';
    valueText: string;
    accentColor: string;
    className?: string;
}

const SCROLL_DURATION_MS = 1950;
const Y_TICKS = [100, 75, 50, 25, 0];
const PADDING = { top: 8, right: 8, bottom: 8, left: 44 } as const;

interface Point {
    x: number;
    y: number;
}

// 在 x = clipX 处对相邻两点线性插值，得到新点（用于左右边界裁切）。
// 对应 legacy `_interpolate_point_at_x`。
function interpolateAtX(a: Point, b: Point, clipX: number): Point {
    if (a.x === b.x) return { x: clipX, y: b.y };
    const r = (clipX - a.x) / (b.x - a.x);
    return { x: clipX, y: a.y + (b.y - a.y) * r };
}

// 把超出 [left, right] 的点裁掉，端点用插值。
// 对应 legacy `_build_display_points`。
function clipDisplayPoints(points: Point[], left: number, right: number): Point[] {
    if (points.length < 2) return points;
    const out: Point[] = [];
    if (points[0].x < left) {
        out.push(interpolateAtX(points[0], points[1], left));
    }
    for (const p of points) {
        if (p.x >= left && p.x <= right) out.push(p);
    }
    const last = points[points.length - 1];
    if (last.x > right) {
        out.push(interpolateAtX(points[points.length - 2], last, right));
    }
    return out.length >= 2 ? out : points;
}

// 平滑曲线 path：每段 cubic bezier 控制点 = 段中点垂直竖直处。
// 对应 legacy `_build_smooth_path`。
function buildSmoothPath(points: Point[]): string {
    if (points.length === 0) return '';
    if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;
    let d = `M ${points[0].x} ${points[0].y}`;
    for (let i = 1; i < points.length; i++) {
        const prev = points[i - 1];
        const cur = points[i];
        const midX = (prev.x + cur.x) / 2;
        d += ` C ${midX} ${prev.y}, ${midX} ${cur.y}, ${cur.x} ${cur.y}`;
    }
    return d;
}

function buildAreaPath(linePath: string, points: Point[], bottom: number, left: number): string {
    if (points.length === 0) return '';
    const last = points[points.length - 1];
    return `${linePath} L ${last.x} ${bottom} L ${left} ${bottom} Z`;
}

// 根据 hover 鼠标 x，在 clipped 曲线上线性插值得到 (x, y, value)。
// 对应 legacy `_point_and_value_at_x`。
function pickHover(
    clipped: Point[],
    values: number[],
    rawCount: number,
    hoverX: number,
): { p: Point; value: number } | null {
    if (clipped.length < 2) return null;
    if (hoverX <= clipped[0].x) {
        return { p: clipped[0], value: values[0] };
    }
    if (hoverX >= clipped[clipped.length - 1].x) {
        return {
            p: clipped[clipped.length - 1],
            value: values[Math.min(values.length - 1, rawCount - 1)],
        };
    }
    for (let i = 1; i < clipped.length; i++) {
        const a = clipped[i - 1];
        const b = clipped[i];
        if (hoverX > b.x) continue;
        if (b.x === a.x) return { p: { x: hoverX, y: b.y }, value: values[Math.min(i, values.length - 1)] };
        const r = (hoverX - a.x) / (b.x - a.x);
        const y = a.y + (b.y - a.y) * r;
        // value 通过 raw 序列插值更自然
        const valueAIdx = Math.max(0, Math.min(values.length - 1, i - 1));
        const valueBIdx = Math.max(0, Math.min(values.length - 1, i));
        const va = values[valueAIdx];
        const vb = values[valueBIdx];
        return { p: { x: hoverX, y }, value: va + (vb - va) * r };
    }
    return null;
}

export const OccupancyChart: React.FC<OccupancyChartProps> = ({
    title,
    icon: Icon,
    history,
    dataKey,
    valueText,
    accentColor,
    className,
}) => {
    const gradientId = `occupancy-gradient-${title.toLowerCase()}`;

    const wrapperRef = useRef<HTMLDivElement | null>(null);
    const [size, setSize] = useState({ w: 0, h: 0 });

    const valuesPrevRef = useRef<number[]>(history.map((p) => p[dataKey]));
    const [progress, setProgress] = useState(1);
    const lastTickRef = useRef<number | null>(null);

    // hover 状态。null = 未 hover。
    const [hoverX, setHoverX] = useState<number | null>(null);

    useEffect(() => {
        const el = wrapperRef.current;
        if (!el) return;
        const update = () => setSize({ w: el.clientWidth, h: el.clientHeight });
        update();
        const ro = new ResizeObserver(update);
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    useEffect(() => {
        const latest = history[history.length - 1];
        if (!latest) return;
        if (lastTickRef.current === null) {
            lastTickRef.current = latest.t;
            valuesPrevRef.current = history.map((p) => p[dataKey]);
            setProgress(1);
            return;
        }
        if (latest.t === lastTickRef.current) return;
        lastTickRef.current = latest.t;

        setProgress(0);
        const startTime = performance.now();
        let rafId = 0;
        const loop = (now: number) => {
            const elapsed = now - startTime;
            const p = Math.min(1, elapsed / SCROLL_DURATION_MS);
            setProgress(p);
            if (p < 1) {
                rafId = requestAnimationFrame(loop);
            } else {
                valuesPrevRef.current = history.map((pt) => pt[dataKey]);
            }
        };
        rafId = requestAnimationFrame(loop);
        return () => cancelAnimationFrame(rafId);
    }, [history, dataKey]);

    const innerW = Math.max(0, size.w - PADDING.left - PADDING.right);
    const innerH = Math.max(0, size.h - PADDING.top - PADDING.bottom);
    const isAnimating = progress < 1;

    const valuesPrev = valuesPrevRef.current;
    const latestValue = history[history.length - 1]?.[dataKey] ?? 0;
    const renderValues = useMemo(
        () => (isAnimating ? [...valuesPrev, latestValue] : valuesPrev),
        [isAnimating, valuesPrev, latestValue],
    );

    const N = valuesPrev.length;
    const stepX = N > 1 ? innerW / (N - 1) : innerW;
    const shift = isAnimating ? stepX * progress : 0;

    const rawPoints: Point[] = useMemo(
        () =>
            renderValues.map((v, i) => ({
                x: i * stepX - shift,
                y: innerH * (1 - v / 100),
            })),
        [renderValues, stepX, shift, innerH],
    );
    const clipped = useMemo(
        () => clipDisplayPoints(rawPoints, 0, innerW),
        [rawPoints, innerW],
    );
    const linePath = useMemo(() => buildSmoothPath(clipped), [clipped]);
    const areaPath = useMemo(
        () => buildAreaPath(linePath, clipped, innerH, 0),
        [linePath, clipped, innerH],
    );

    // hover：把鼠标 x（屏幕坐标）映射到内绘图区坐标
    const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
        const el = wrapperRef.current;
        if (!el) return;
        const rect = el.getBoundingClientRect();
        const localX = e.clientX - rect.left - PADDING.left;
        if (localX < 0 || localX > innerW) {
            setHoverX(null);
            return;
        }
        setHoverX(localX);
    };
    const handlePointerLeave = () => setHoverX(null);

    const hoverInfo = hoverX !== null
        ? pickHover(clipped, renderValues, renderValues.length, hoverX)
        : null;

    // hover 时卡片右上 valueText 切到 hover 值
    const headerValueText = hoverInfo ? `${Math.round(hoverInfo.value)}%` : valueText;

    return (
        <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
            <div className="mb-2 flex shrink-0 items-center gap-2">
                <Icon size={14} strokeWidth={1.75} className="text-text-secondary" />
                <span className="flex-1 text-[13px] font-medium text-text-secondary">{title}</span>
                <span
                    className="font-mono text-[14px] font-semibold tabular-nums"
                    style={{ color: accentColor }}
                >
                    {headerValueText}
                </span>
            </div>

            <div
                ref={wrapperRef}
                className="relative min-h-[140px] flex-1"
                onPointerMove={handlePointerMove}
                onPointerLeave={handlePointerLeave}
            >
                {/* 静态层：Y 轴刻度文字 + 横向虚线网格。
                    用绝对定位贴在 [PADDING.top, h - PADDING.bottom] 之间。 */}
                {size.h > 0 && (
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0"
                        style={{ top: PADDING.top, height: innerH }}
                    >
                        {Y_TICKS.map((tick) => {
                            const yRatio = (100 - tick) / 100;
                            return (
                                <div
                                    key={tick}
                                    className="absolute left-0 right-0 flex items-center"
                                    style={{ top: `calc(${yRatio * 100}% - 7px)` }}
                                >
                                    <span
                                        className="shrink-0 pr-2 text-right font-mono text-[11px] tabular-nums text-text-disabled"
                                        style={{ width: PADDING.left - 4 }}
                                    >
                                        {tick}%
                                    </span>
                                    <div className="flex-1 border-t border-dashed border-border-subtle/70" />
                                </div>
                            );
                        })}
                    </div>
                )}

                {/* 滚动层：自绘 SVG */}
                {size.w > 0 && size.h > 0 && (
                    <svg
                        width={size.w}
                        height={size.h}
                        className="absolute inset-0 overflow-hidden"
                    >
                        <defs>
                            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                                <stop offset="0%" stopColor={accentColor} stopOpacity={0.32} />
                                <stop offset="100%" stopColor={accentColor} stopOpacity={0.02} />
                            </linearGradient>
                        </defs>

                        <g transform={`translate(${PADDING.left}, ${PADDING.top})`}>
                            {areaPath && <path d={areaPath} fill={`url(#${gradientId})`} />}
                            {linePath && (
                                <path
                                    d={linePath}
                                    fill="none"
                                    stroke={accentColor}
                                    strokeWidth={2}
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                />
                            )}

                            {/* hover 指示器 */}
                            {hoverInfo && (
                                <HoverIndicator
                                    point={hoverInfo.p}
                                    valueText={`${Math.round(hoverInfo.value)}%`}
                                    accentColor={accentColor}
                                    chartHeight={innerH}
                                />
                            )}
                        </g>
                    </svg>
                )}
            </div>
        </Card>
    );
};

interface HoverIndicatorProps {
    point: Point;
    valueText: string;
    accentColor: string;
    chartHeight: number;
}

const HoverIndicator: React.FC<HoverIndicatorProps> = ({
    point,
    valueText,
    accentColor,
    chartHeight,
}) => {
    // 上方 pill 高度 + padding 估算
    const pillHeight = 18;
    const pillY = Math.max(0, point.y - pillHeight - 10);
    return (
        <g pointerEvents="none">
            {/* 垂直引导线：从顶到点 */}
            <line
                x1={point.x}
                x2={point.x}
                y1={pillY + pillHeight + 2}
                y2={chartHeight}
                stroke={accentColor}
                strokeOpacity={0.3}
                strokeWidth={1}
                strokeDasharray="3 3"
            />

            {/* 双层圆点：外环 brand 色，内圈白色 */}
            <circle cx={point.x} cy={point.y} r={6} fill={accentColor} fillOpacity={0.18} />
            <circle
                cx={point.x}
                cy={point.y}
                r={4}
                fill={accentColor}
                stroke="var(--surface-card)"
                strokeWidth={2}
            />

            {/* 顶部数值 pill：用 svg foreignObject 容易因尺寸不准而抖动，
                直接画 rect + text 更稳定。文字宽度按字符数粗算。 */}
            <PillLabel
                x={point.x}
                y={pillY}
                height={pillHeight}
                color={accentColor}
                text={valueText}
            />
        </g>
    );
};

const PillLabel: React.FC<{
    x: number;
    y: number;
    height: number;
    color: string;
    text: string;
}> = ({ x, y, height, color, text }) => {
    // 估算 pill 宽：字号 11px, 字符宽约 6.5px, 左右 padding 各 6px
    const pillWidth = text.length * 6.8 + 12;
    return (
        <g pointerEvents="none">
            <rect
                x={x - pillWidth / 2}
                y={y}
                width={pillWidth}
                height={height}
                rx={height / 2}
                fill={color}
            />
            <text
                x={x}
                y={y + height / 2}
                textAnchor="middle"
                dominantBaseline="central"
                fontFamily="var(--font-mono)"
                fontSize={11}
                fontWeight={600}
                fill="#fff"
            >
                {text}
            </text>
        </g>
    );
};

export default OccupancyChart;
