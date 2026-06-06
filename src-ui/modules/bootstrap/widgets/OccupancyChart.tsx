// CPU / RAM 占用率折线图（显示池 + 可选滚入动画）。

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
const PADDING = { top: 8, right: 8, bottom: 8, left: 44 } as const;

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
                onPointerLeave={() => setHoverX(null)}
            >
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

                {size.w > 0 && size.h > 0 && renderValues.length > 0 && (
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
                strokeOpacity={0.3}
                strokeWidth={1}
                strokeDasharray="3 3"
            />
            <circle cx={point.x} cy={point.y} r={6} fill={accentColor} fillOpacity={0.18} />
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