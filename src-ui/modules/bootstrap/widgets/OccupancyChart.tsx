// CPU / RAM 占用率折线图（渐变填充 + 发光曲线 + 呼吸端点 + 新点滚入）。
//
// 监控默认开着，间隔 1.2 s、滚一次 1.18 s，以前逐帧重算裁剪后的路径，等于首页一直在重绘这两张图，
// 发光滤镜还要跟着每帧重新栅格化。现在每个采样只算一次 N+1 个点的整条曲线，外层 div 用 WAAPI
// 线性左移一格、绘图区 overflow 裁掉两端，滚动交给合成线程；端点圆点在 HTML 层单独动。

import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import { Card } from '../../../shared/ui';
import type { ResourcePoint } from '../../../hooks/diagnostics/useResourceMonitor';
import { performanceScrollDurationMs } from '../../../core/domain/performance/performanceSettings';
import {
    buildAreaPath,
    buildSmoothPath,
    clipDisplayPoints,
    EDGE_EASE,
    pickHover,
    scrollPoints,
    steadyPoints,
} from './occupancyChartGeometry';
import { useOccupancySegment } from './useOccupancySegment';

interface OccupancyChartProps {
    title: string;
    icon: LucideIcon;
    history: ResourcePoint[];
    dataKey: 'cpu' | 'ram';
    valueText: string;
    accentColor: string;
    sampleIntervalMs: number;
    motionEnabled: boolean;
    className?: string;
}

const Y_TICKS = [100, 75, 50, 25, 0];
const PADDING = { top: 8, right: 8, bottom: 8, left: 38 } as const;

const valueY = (value: number, innerH: number) => innerH * (1 - value / 100);

// 滚入进度按时间算，和 WAAPI 那边是同一个起点，最多差一帧
function scrollProgress(startedAt: number, duration: number): number {
    return Math.min(1, Math.max(0, (performance.now() - startedAt) / duration));
}

export const OccupancyChart: React.FC<OccupancyChartProps> = ({
    title,
    icon: Icon,
    history,
    dataKey,
    valueText,
    accentColor,
    sampleIntervalMs,
    motionEnabled,
    className,
}) => {
    const gradientId = `occupancy-gradient-${title.toLowerCase()}`;
    const filterId = `occupancy-glow-${title.toLowerCase()}`;
    const wrapperRef = useRef<HTMLDivElement | null>(null);
    const trackRef = useRef<HTMLDivElement | null>(null);
    const dotRef = useRef<HTMLSpanElement | null>(null);
    const animsRef = useRef<Animation[]>([]);
    const [size, setSize] = useState({ w: 0, h: 0 });
    const [hoverX, setHoverX] = useState<number | null>(null);

    const segment = useOccupancySegment(history, dataKey, motionEnabled);
    const duration = performanceScrollDurationMs(sampleIntervalMs);
    // 滚到一半关了动效，这一段直接落到稳态
    const scrollFrom = motionEnabled ? segment.scrollFrom : null;

    useEffect(() => {
        const el = wrapperRef.current;
        if (!el) return;
        const update = () => setSize({ w: el.clientWidth, h: el.clientHeight });
        update();
        const ro = new ResizeObserver(update);
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    const innerW = Math.max(0, size.w - PADDING.left - PADDING.right);
    const innerH = Math.max(0, size.h - PADDING.top - PADDING.bottom);
    const { steady, startedAt } = segment;
    const slots = steady.length;
    const stepX = slots > 1 ? innerW / (slots - 1) : innerW;
    const incoming = steady[slots - 1];

    // 这一段的整条曲线，不裁剪：滚入时 N+1 个点，最右边的新点先待在绘图区外面
    const { values, points } = useMemo(() => {
        if (scrollFrom) return scrollPoints(scrollFrom, incoming, 0, slots, innerW, innerH);
        return { values: steady, points: steadyPoints(steady, innerW, innerH) };
    }, [scrollFrom, steady, incoming, slots, innerW, innerH]);
    const linePath = useMemo(() => buildSmoothPath(points), [points]);
    const areaPath = useMemo(
        () => buildAreaPath(linePath, points, innerH, 0),
        [linePath, points, innerH],
    );

    useLayoutEffect(() => {
        for (const anim of animsRef.current) anim.cancel();
        animsRef.current = [];
        const track = trackRef.current;
        const dot = dotRef.current;
        if (!scrollFrom || !track || !dot || typeof track.animate !== 'function' || innerW <= 0) return;

        const timing: KeyframeAnimationOptions = { duration, fill: 'forwards' };
        const fromY = valueY(scrollFrom[scrollFrom.length - 1], innerH);
        const toY = valueY(incoming, innerH);
        const anims = [
            track.animate(
                [{ transform: 'translateX(0)' }, { transform: `translateX(${-stepX}px)` }],
                { ...timing, easing: 'linear' },
            ),
            // 端点贴着绘图区右边缘，沿「旧的最后一点 → 新点」这段曲线上下走
            dot.animate(
                [
                    { transform: `translate(${innerW}px, ${fromY}px)` },
                    { transform: `translate(${innerW}px, ${toY}px)` },
                ],
                { ...timing, easing: EDGE_EASE },
            ),
        ];
        // 换段时 elapsed 接近 0；尺寸变了重建动画时从已经播过的位置接着走
        const elapsed = Math.min(duration, Math.max(0, performance.now() - startedAt));
        for (const anim of anims) anim.currentTime = elapsed;
        animsRef.current = anims;
    }, [scrollFrom, startedAt, incoming, duration, stepX, innerW, innerH]);

    useEffect(
        () => () => {
            for (const anim of animsRef.current) anim.cancel();
        },
        [],
    );

    // 悬停时十字线要跟着正在滚的曲线走：只在这时逐帧刷新，隔帧提交，滚完就停
    const hovering = hoverX !== null;
    const [, setHoverTick] = useState(0);
    useEffect(() => {
        if (!hovering || !scrollFrom) return;
        let raf = 0;
        let skip = false;
        const loop = () => {
            const done = scrollProgress(startedAt, duration) >= 1;
            skip = !skip;
            if (done || !skip) setHoverTick((n) => n + 1);
            if (!done) raf = requestAnimationFrame(loop);
        };
        raf = requestAnimationFrame(loop);
        return () => cancelAnimationFrame(raf);
    }, [hovering, scrollFrom, startedAt, duration]);

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

    let hoverInfo: ReturnType<typeof pickHover> = null;
    if (hovering && slots > 0) {
        const live = scrollFrom
            ? scrollPoints(scrollFrom, incoming, scrollProgress(startedAt, duration), slots, innerW, innerH)
            : { values, points };
        hoverInfo = pickHover(clipDisplayPoints(live.points, 0, innerW), live.values, hoverX);
    }
    const headerValueText = hoverInfo ? `${Math.round(hoverInfo.value)}%` : valueText;
    const ready = size.w > 0 && size.h > 0 && slots > 0;

    return (
        <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
            {/* 卡片头部：图标 + 标题 + 醒目大字号当前负载 */}
            <div className="mb-2 flex shrink-0 items-center justify-between">
                <div className="flex items-center gap-2">
                    <div
                        className="grid h-7 w-7 place-items-center rounded-sm"
                        style={{ backgroundColor: `color-mix(in srgb, ${accentColor} 14%, transparent)` }}
                    >
                        <Icon size={14} strokeWidth={2} style={{ color: accentColor }} />
                    </div>
                    <div className="flex items-center gap-1.5">
                        <span className="text-xs font-semibold text-text">{title}</span>
                        <span
                            className="h-1.5 w-1.5 rounded-full animate-pulse"
                            style={{ backgroundColor: accentColor }}
                        />
                    </div>
                </div>
                <div className="flex items-baseline gap-1">
                    <span
                        className="font-mono text-lg font-bold tabular-nums tracking-tight"
                        style={{ color: accentColor }}
                    >
                        {headerValueText}
                    </span>
                </div>
            </div>

            <div
                ref={wrapperRef}
                className="relative min-h-[135px] flex-1 cursor-crosshair"
                onPointerMove={handlePointerMove}
                onPointerLeave={() => setHoverX(null)}
            >
                {/* 刻度虚线 */}
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
                                    style={{ top: `calc(${yRatio * 100}% - 6px)` }}
                                >
                                    <span
                                        className="shrink-0 pr-1.5 text-right font-mono text-[10px] tabular-nums text-text-tertiary/60 select-none"
                                        style={{ width: PADDING.left - 4 }}
                                    >
                                        {tick}%
                                    </span>
                                    <div className="flex-1 border-t border-dashed border-border-subtle/40" />
                                </div>
                            );
                        })}
                    </div>
                )}

                {ready && (
                    <>
                        {/* 绘图区：裁掉已经滚出左边、还没滚进右边的部分 */}
                        <div
                            aria-hidden
                            className="pointer-events-none absolute top-0 overflow-hidden"
                            style={{ left: PADDING.left, width: innerW, height: size.h }}
                        >
                            <div ref={trackRef} className="absolute left-0 top-0 will-change-transform">
                                <svg width={innerW + stepX} height={size.h} className="block">
                                    <defs>
                                        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                                            <stop offset="0%" stopColor={accentColor} stopOpacity={0.36} />
                                            <stop offset="45%" stopColor={accentColor} stopOpacity={0.12} />
                                            <stop offset="100%" stopColor={accentColor} stopOpacity={0.00} />
                                        </linearGradient>

                                        {/* 发光滤镜用 userSpaceOnUse：水平直线的包围盒高度为 0，按对象算区域整条线会被裁没 */}
                                        <filter
                                            id={filterId}
                                            x="0"
                                            y="0"
                                            width={innerW + stepX}
                                            height={size.h}
                                            filterUnits="userSpaceOnUse"
                                        >
                                            <feDropShadow
                                                dx="0"
                                                dy="1.5"
                                                stdDeviation="2.5"
                                                floodColor={accentColor}
                                                floodOpacity="0.4"
                                            />
                                        </filter>
                                    </defs>

                                    <g transform={`translate(0, ${PADDING.top})`}>
                                        {areaPath && <path d={areaPath} fill={`url(#${gradientId})`} />}
                                        {linePath && (
                                            <path
                                                d={linePath}
                                                fill="none"
                                                stroke={accentColor}
                                                strokeWidth={2.2}
                                                strokeLinecap="round"
                                                strokeLinejoin="round"
                                                filter={`url(#${filterId})`}
                                            />
                                        )}
                                    </g>
                                </svg>
                            </div>
                        </div>

                        {/* 最新数据点；悬停时让位给十字线，但不卸载，免得丢了正在播的动画 */}
                        <span
                            ref={dotRef}
                            aria-hidden
                            className="pointer-events-none absolute"
                            style={{
                                left: PADDING.left,
                                top: PADDING.top,
                                transform: `translate(${innerW}px, ${valueY(incoming, innerH)}px)`,
                                opacity: hoverInfo ? 0 : 1,
                            }}
                        >
                            <span
                                className={`absolute -left-1.5 -top-1.5 h-3 w-3 rounded-full${motionEnabled ? ' animate-pulse' : ''}`}
                                style={{ backgroundColor: `color-mix(in srgb, ${accentColor} 25%, transparent)` }}
                            />
                            <span
                                className="absolute rounded-full"
                                style={{
                                    left: -4.4,
                                    top: -4.4,
                                    width: 8.8,
                                    height: 8.8,
                                    backgroundColor: accentColor,
                                    border: '1.8px solid var(--surface-card)',
                                }}
                            />
                        </span>

                        {hoverInfo && (
                            <svg
                                width={size.w}
                                height={size.h}
                                className="pointer-events-none absolute inset-0 overflow-hidden"
                            >
                                <g transform={`translate(${PADDING.left}, ${PADDING.top})`}>
                                    <HoverIndicator
                                        point={hoverInfo.p}
                                        valueText={`${Math.round(hoverInfo.value)}%`}
                                        accentColor={accentColor}
                                        chartHeight={innerH}
                                    />
                                </g>
                            </svg>
                        )}
                    </>
                )}
            </div>
        </Card>
    );
};

const HoverIndicator: React.FC<{
    point: { x: number; y: number };
    valueText: string;
    accentColor: string;
    chartHeight: number;
}> = ({ point, valueText, accentColor, chartHeight }) => {
    const pillHeight = 18;
    const pillY = Math.max(0, point.y - pillHeight - 10);
    return (
        <g pointerEvents="none">
            <line
                x1={point.x}
                x2={point.x}
                y1={pillY + pillHeight + 2}
                y2={chartHeight}
                stroke={accentColor}
                strokeOpacity={0.4}
                strokeWidth={1}
                strokeDasharray="3 3"
            />
            <circle cx={point.x} cy={point.y} r={7} fill={accentColor} fillOpacity={0.2} />
            <circle
                cx={point.x}
                cy={point.y}
                r={4}
                fill={accentColor}
                stroke="var(--surface-card)"
                strokeWidth={2}
            />
            <PillLabel x={point.x} y={pillY} height={pillHeight} color={accentColor} text={valueText} />
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
    const pillWidth = text.length * 6.8 + 14;
    return (
        <g pointerEvents="none">
            <rect
                x={x - pillWidth / 2}
                y={y}
                width={pillWidth}
                height={height}
                rx={height / 2}
                fill={color}
                className="drop-shadow-xs"
            />
            <text
                x={x}
                y={y + height / 2}
                textAnchor="middle"
                dominantBaseline="central"
                fontFamily="var(--font-mono)"
                fontSize={10.5}
                fontWeight={600}
                fill="#fff"
            >
                {text}
            </text>
        </g>
    );
};

export default OccupancyChart;