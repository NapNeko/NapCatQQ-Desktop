// CPU / RAM 占用率折线图（高质感渐变光晕 + 实时呼吸端点 + 滚入平滑动画）。

import React, { useEffect, useMemo, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import { Card } from '../../../shared/ui';
import type { ResourcePoint } from '../../../hooks/diagnostics/useResourceMonitor';
import {
    buildAreaPath,
    buildSmoothPath,
    clipDisplayPoints,
    pickHover,
    scrollPoints,
    steadyPoints,
} from './occupancyChartGeometry';
import { useOccupancyScrollAnimation } from './useOccupancyScrollAnimation';

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
    const [size, setSize] = useState({ w: 0, h: 0 });
    const [hoverX, setHoverX] = useState<number | null>(null);

    const frame = useOccupancyScrollAnimation(
        history,
        dataKey,
        sampleIntervalMs,
        motionEnabled,
    );

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

    const { renderValues, rawPoints } = useMemo(() => {
        if (frame.mode === 'scroll' && frame.scroll) {
            const { animationSource, incoming, steadySlotCount, progress } = frame.scroll;
            const built = scrollPoints(
                animationSource,
                incoming,
                progress,
                steadySlotCount,
                innerW,
                innerH,
            );
            return { renderValues: built.values, rawPoints: built.points };
        }
        return {
            renderValues: frame.values,
            rawPoints: steadyPoints(frame.values, innerW, innerH),
        };
    }, [frame, innerW, innerH]);

    const clipped = useMemo(
        () => clipDisplayPoints(rawPoints, 0, innerW),
        [rawPoints, innerW],
    );
    const linePath = useMemo(() => buildSmoothPath(clipped), [clipped]);
    const areaPath = useMemo(
        () => buildAreaPath(linePath, clipped, innerH, 0),
        [linePath, clipped, innerH],
    );

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

    const hoverInfo =
        hoverX !== null ? pickHover(clipped, renderValues, hoverX) : null;
    const headerValueText = hoverInfo ? `${Math.round(hoverInfo.value)}%` : valueText;
    const latestPoint = clipped.length > 0 ? clipped[clipped.length - 1] : null;

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

                {/* 图表主体 */}
                {size.w > 0 && size.h > 0 && renderValues.length > 0 && (
                    <svg
                        width={size.w}
                        height={size.h}
                        className="absolute inset-0 overflow-hidden"
                    >
                        <defs>
                            {/* 渐变遮罩 */}
                            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                                <stop offset="0%" stopColor={accentColor} stopOpacity={0.36} />
                                <stop offset="45%" stopColor={accentColor} stopOpacity={0.12} />
                                <stop offset="100%" stopColor={accentColor} stopOpacity={0.00} />
                            </linearGradient>

                            {/* 霓虹发光滤镜：使用 userSpaceOnUse 避免水平直线时 height=0 导致整条线被浏览器裁切消失 */}
                            <filter
                                id={filterId}
                                x="0"
                                y="0"
                                width={size.w}
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

                        <g transform={`translate(${PADDING.left}, ${PADDING.top})`}>
                            {/* 面积渐变填充 */}
                            {areaPath && <path d={areaPath} fill={`url(#${gradientId})`} />}

                            {/* 发光曲线 */}
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

                            {/* 实时最新数据点发光指示 */}
                            {!hoverInfo && latestPoint && (
                                <g transform={`translate(${latestPoint.x}, ${latestPoint.y})`}>
                                    <circle
                                        r={6}
                                        fill={accentColor}
                                        fillOpacity={0.25}
                                        className={motionEnabled ? 'animate-pulse' : undefined}
                                    />
                                    <circle
                                        r={3.5}
                                        fill={accentColor}
                                        stroke="var(--surface-card)"
                                        strokeWidth={1.8}
                                    />
                                </g>
                            )}

                            {/* 鼠标悬停十字标尺 */}
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