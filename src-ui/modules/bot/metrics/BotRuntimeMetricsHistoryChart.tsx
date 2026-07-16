// 指标历史折线。
// 默认 Y 轴从 0 起；可选 fit 贴合数据区间。交互对齐概览 OccupancyChart：悬浮读数 + 十字线。

import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { MetricsHistoryPoint } from '../../../core/ipc/generated/domain/MetricsHistoryPoint';
import {
    buildAreaPath,
    buildSmoothPath,
    clipDisplayPoints,
    type ChartPoint,
} from '../../bootstrap/widgets/occupancyChartGeometry';
import { formatBytes, formatCompactCount } from '../../../core/domain/bot/runtime-metrics-settings';
import { cn } from '../../../shared/utils/cn';

export type HistorySeriesKey = 'rss' | 'eventsOut' | 'actionsIn';
export type ChartScaleMode = 'zero' | 'fit';

export interface BotRuntimeMetricsHistoryChartProps {
    points: MetricsHistoryPoint[];
    series: HistorySeriesKey;
    accentColor: string;
    title: string;
    emptyHint?: string;
    className?: string;
    /** zero: 从 0 到 max（默认）；fit: 贴合数据区间 */
    scaleMode?: ChartScaleMode;
    showDots?: boolean;
}

const PADDING = { top: 10, right: 12, bottom: 22, left: 52 } as const;

function seriesValues(points: MetricsHistoryPoint[], series: HistorySeriesKey): number[] {
    return points.map((p) => {
        if (series === 'rss') {
            const v = p.memory?.rss_bytes;
            return v != null && Number.isFinite(Number(v)) ? Number(v) : 0;
        }
        if (series === 'eventsOut') {
            return Number(p.nodes_summary?.events_out_total ?? 0) || 0;
        }
        return Number(p.nodes_summary?.actions_in_total ?? 0) || 0;
    });
}

function formatY(series: HistorySeriesKey, v: number): string {
    if (series === 'rss') return formatBytes(v);
    return formatCompactCount(v);
}

function formatChartTime(atMs: number | null | undefined): string {
    if (atMs == null || !Number.isFinite(Number(atMs))) return '—';
    return new Intl.DateTimeFormat('zh-CN', {
        month: 'numeric',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    }).format(new Date(Number(atMs)));
}

function niceStep(span: number): number {
    if (!Number.isFinite(span) || span <= 0) return 1;
    const exp = Math.floor(Math.log10(span));
    const base = Math.pow(10, exp);
    const n = span / base;
    if (n <= 1.2) return base * 0.2;
    if (n <= 2.5) return base * 0.5;
    if (n <= 5) return base;
    return base * 2;
}

function niceCeil(value: number, series: HistorySeriesKey): number {
    if (!Number.isFinite(value) || value <= 0) {
        return series === 'rss' ? 64 * 1024 * 1024 : 10;
    }
    if (series === 'rss') {
        const mb = value / (1024 * 1024);
        // 用常见内存刻度，避免 0~209 这种怪上限
        if (mb <= 32) return 32 * 1024 * 1024;
        if (mb <= 64) return 64 * 1024 * 1024;
        if (mb <= 128) return 128 * 1024 * 1024;
        if (mb <= 256) return 256 * 1024 * 1024;
        if (mb <= 384) return 384 * 1024 * 1024;
        if (mb <= 512) return 512 * 1024 * 1024;
        if (mb <= 768) return 768 * 1024 * 1024;
        if (mb <= 1024) return 1024 * 1024 * 1024;
        return Math.ceil(mb / 256) * 256 * 1024 * 1024;
    }
    const step = niceStep(value);
    return Math.ceil(value / step) * step;
}

function computeAxisDomain(
    values: number[],
    series: HistorySeriesKey,
    scaleMode: ChartScaleMode,
): { min: number; max: number } {
    const finite = values.filter((v) => Number.isFinite(v));
    if (finite.length === 0) return { min: 0, max: series === 'rss' ? 64 * 1024 * 1024 : 10 };

    const dataMin = Math.min(...finite);
    const dataMax = Math.max(...finite);

    if (scaleMode === 'zero') {
        // 从 0 起；RSS 用固定友好上限，计数用 nice 上限 + 头顶余量
        if (series === 'rss') {
            return { min: 0, max: niceCeil(dataMax * 1.08, series) };
        }
        const max = niceCeil(Math.max(dataMax * 1.15, 1), series);
        return { min: 0, max: Math.max(max, 1) };
    }

    // fit：贴合数据区间，看小波动
    let min = dataMin;
    let max = dataMax;
    if (min === max) {
        if (max === 0) return { min: 0, max: series === 'rss' ? 16 * 1024 * 1024 : 1 };
        const pad = Math.abs(max) * 0.12;
        min = Math.max(0, max - pad);
        max = max + pad;
    } else {
        const span = max - min;
        const pad = Math.max(span * 0.18, Math.abs(max) * 0.03);
        min = Math.max(0, min - pad * 0.2);
        max = max + pad;
    }

    if (series === 'rss') {
        // fit 也尽量落到 MB 整数边界
        const minMb = Math.floor(min / (1024 * 1024));
        const maxMb = Math.ceil(max / (1024 * 1024));
        min = Math.max(0, minMb) * 1024 * 1024;
        max = Math.max(min + 1024 * 1024, maxMb * 1024 * 1024);
        return { min, max };
    }

    const step = niceStep(max - min);
    min = Math.floor(min / step) * step;
    max = Math.ceil(max / step) * step;
    if (min < 0) min = 0;
    if (max <= min) max = min + step;
    return { min, max };
}

function buildYTicks(min: number, max: number, series: HistorySeriesKey): number[] {
    if (!(max > min)) return [min];
    if (series === 'rss') {
        // 4 档：0 / 1/3 / 2/3 / max，读起来比只有 3 档稳
        return [max, min + (max - min) * (2 / 3), min + (max - min) / 3, min];
    }
    return [max, min + (max - min) * 0.5, min];
}

function pointsFromValues(
    values: number[],
    innerW: number,
    innerH: number,
    minV: number,
    maxV: number,
): ChartPoint[] {
    const n = values.length;
    if (n === 0) return [];
    const span = Math.max(1e-9, maxV - minV);
    const stepX = n > 1 ? innerW / (n - 1) : innerW;
    return values.map((v, i) => {
        const t = (v - minV) / span;
        return {
            x: n > 1 ? i * stepX : innerW,
            y: innerH * (1 - Math.min(1, Math.max(0, t))),
        };
    });
}

function nearestIndex(clipped: ChartPoint[], hoverX: number): number | null {
    if (clipped.length === 0) return null;
    let best = 0;
    let bestD = Infinity;
    for (let i = 0; i < clipped.length; i++) {
        const d = Math.abs(clipped[i].x - hoverX);
        if (d < bestD) {
            bestD = d;
            best = i;
        }
    }
    return best;
}

function HoverIndicator({
    point,
    valueText,
    timeText,
    accentColor,
    chartHeight,
    chartWidth,
}: {
    point: ChartPoint;
    valueText: string;
    timeText: string;
    accentColor: string;
    chartHeight: number;
    chartWidth: number;
}) {
    const pillText = `${valueText} · ${timeText}`;
    const pillHeight = 20;
    const pillWidth = Math.min(chartWidth - 8, Math.max(72, pillText.length * 6.4 + 16));
    let pillX = point.x - pillWidth / 2;
    pillX = Math.max(0, Math.min(chartWidth - pillWidth, pillX));
    const pillY = Math.max(0, Math.min(chartHeight - pillHeight - 4, point.y - pillHeight - 12));

    return (
        <g pointerEvents="none">
            <line
                x1={point.x}
                x2={point.x}
                y1={0}
                y2={chartHeight}
                stroke={accentColor}
                strokeOpacity={0.28}
                strokeWidth={1}
                strokeDasharray="3 3"
            />
            <circle cx={point.x} cy={point.y} r={6.5} fill={accentColor} fillOpacity={0.16} />
            <circle
                cx={point.x}
                cy={point.y}
                r={3.5}
                fill={accentColor}
                stroke="var(--surface-card)"
                strokeWidth={1.75}
            />
            <rect
                x={pillX}
                y={pillY}
                width={pillWidth}
                height={pillHeight}
                rx={pillHeight / 2}
                fill={accentColor}
            />
            <text
                x={pillX + pillWidth / 2}
                y={pillY + pillHeight / 2 + 0.5}
                textAnchor="middle"
                dominantBaseline="middle"
                fill="#fff"
                fontSize={10}
                fontFamily="var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace)"
                fontWeight={600}
            >
                {pillText}
            </text>
        </g>
    );
}

export function BotRuntimeMetricsHistoryChart({
    points,
    series,
    accentColor,
    title,
    emptyHint = '尚无采样点；运行一段时间后出现（约 ≥1 分钟一点）',
    className,
    scaleMode = 'zero',
    showDots = false,
}: BotRuntimeMetricsHistoryChartProps) {
    const wrapperRef = useRef<HTMLDivElement | null>(null);
    const [size, setSize] = useState({ w: 0, h: 0 });
    const [hoverX, setHoverX] = useState<number | null>(null);
    const reactId = useId().replace(/:/g, '');
    const gradientId = `metrics-hist-${reactId}`;

    useEffect(() => {
        const el = wrapperRef.current;
        if (!el) return;
        const update = () => setSize({ w: el.clientWidth, h: el.clientHeight });
        update();
        const ro = new ResizeObserver(update);
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    const values = useMemo(() => seriesValues(points, series), [points, series]);
    const domain = useMemo(
        () => computeAxisDomain(values, series, scaleMode),
        [values, series, scaleMode],
    );
    const innerW = Math.max(0, size.w - PADDING.left - PADDING.right);
    const innerH = Math.max(0, size.h - PADDING.top - PADDING.bottom);

    const rawPoints = useMemo(
        () => pointsFromValues(values, innerW, innerH, domain.min, domain.max),
        [values, innerW, innerH, domain.min, domain.max],
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

    const yTicks = useMemo(
        () => buildYTicks(domain.min, domain.max, series),
        [domain.min, domain.max, series],
    );

    const hoverIdx = useMemo(() => {
        if (hoverX == null) return null;
        return nearestIndex(clipped, hoverX);
    }, [hoverX, clipped]);

    const latestIdx = values.length > 0 ? values.length - 1 : null;
    const activeIdx = hoverIdx ?? latestIdx;
    const activeValue = activeIdx != null ? values[activeIdx] : null;
    const activePoint = activeIdx != null ? points[activeIdx] : null;
    const activeChartPoint = activeIdx != null ? clipped[activeIdx] : null;

    const headerValue =
        activeValue != null && Number.isFinite(activeValue)
            ? formatY(series, activeValue)
            : '—';
    const activeTime = formatChartTime(activePoint?.at_ms);
    const firstTime = formatChartTime(points[0]?.at_ms);
    const lastTime = formatChartTime(points[points.length - 1]?.at_ms);

    const stats = useMemo(() => {
        if (values.length === 0) return null;
        const finite = values.filter((v) => Number.isFinite(v));
        if (finite.length === 0) return null;
        const min = Math.min(...finite);
        const max = Math.max(...finite);
        const last = finite[finite.length - 1];
        return { min, max, last, count: finite.length };
    }, [values]);

    // 采样点过多时抽稀，避免糊成一片
    const dotStride = Math.max(1, Math.ceil(clipped.length / 40));

    return (
        <div
            className={cn(
                'flex h-full min-h-0 flex-col rounded-md bg-inset/35 p-2',
                className,
            )}
        >
            <div className="mb-1.5 flex shrink-0 items-start justify-between gap-2 px-0.5">
                <div className="min-w-0">
                    <p className="text-2xs font-medium text-text-secondary">{title}</p>
                    {stats ? (
                        <p className="mt-0.5 text-[10px] tabular-nums text-text-tertiary">
                            最低 {formatY(series, stats.min)} · 最高{' '}
                            {formatY(series, stats.max)} · {stats.count} 点
                        </p>
                    ) : (
                        <p className="mt-0.5 text-[10px] text-text-tertiary">等待采样…</p>
                    )}
                </div>
                <div className="shrink-0 text-right">
                    <p
                        className="font-mono text-sm font-semibold tabular-nums leading-none"
                        style={{ color: accentColor }}
                    >
                        {headerValue}
                    </p>
                    <p className="mt-0.5 text-[10px] tabular-nums text-text-tertiary">
                        {hoverIdx != null ? activeTime : `最新 · ${activeTime}`}
                    </p>
                </div>
            </div>

            <div
                ref={wrapperRef}
                className="relative min-h-0 flex-1 touch-manipulation select-none"
                onPointerMove={(e) => {
                    const el = wrapperRef.current;
                    if (!el) return;
                    const rect = el.getBoundingClientRect();
                    const localX = e.clientX - rect.left - PADDING.left;
                    if (localX < 0 || localX > innerW) {
                        setHoverX(null);
                        return;
                    }
                    setHoverX(localX);
                }}
                onPointerLeave={() => setHoverX(null)}
                role="img"
                aria-label={`${title}历史趋势，当前 ${headerValue}`}
            >
                {values.length === 0 ? (
                    <p className="absolute inset-0 flex items-center justify-center px-4 text-center text-2xs text-text-tertiary">
                        {emptyHint}
                    </p>
                ) : null}

                {size.w > 0 && size.h > 0 && values.length > 0 ? (
                    <>
                        <div
                            aria-hidden
                            className="pointer-events-none absolute inset-x-0"
                            style={{ top: PADDING.top, height: innerH }}
                        >
                            {yTicks.map((tick, i) => {
                                const yRatio =
                                    domain.max === domain.min
                                        ? 0
                                        : (domain.max - tick) / (domain.max - domain.min);
                                return (
                                    <div
                                        key={`${tick}-${i}`}
                                        className="absolute left-0 right-0 flex items-center"
                                        style={{ top: `calc(${yRatio * 100}% - 6px)` }}
                                    >
                                        <span
                                            className="shrink-0 pr-1.5 text-right font-mono text-[10px] tabular-nums text-text-tertiary"
                                            style={{ width: PADDING.left - 4 }}
                                        >
                                            {formatY(series, tick)}
                                        </span>
                                        <div className="flex-1 border-t border-dashed border-border-subtle/55" />
                                    </div>
                                );
                            })}
                        </div>
                        <svg
                            width={size.w}
                            height={size.h}
                            className="absolute inset-0 overflow-hidden"
                        >
                            <defs>
                                <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="0%" stopColor={accentColor} stopOpacity={0.26} />
                                    <stop offset="100%" stopColor={accentColor} stopOpacity={0.02} />
                                </linearGradient>
                            </defs>
                            <g transform={`translate(${PADDING.left}, ${PADDING.top})`}>
                                {areaPath ? (
                                    <path d={areaPath} fill={`url(#${gradientId})`} />
                                ) : null}
                                {linePath ? (
                                    <path
                                        d={linePath}
                                        fill="none"
                                        stroke={accentColor}
                                        strokeWidth={2}
                                        strokeLinecap="round"
                                        strokeLinejoin="round"
                                    />
                                ) : null}
                                {showDots
                                    ? clipped.map((p, i) =>
                                        i % dotStride === 0 || i === clipped.length - 1 ? (
                                            <circle
                                                key={i}
                                                cx={p.x}
                                                cy={p.y}
                                                r={1.8}
                                                fill={accentColor}
                                                fillOpacity={0.85}
                                            />
                                        ) : null,
                                    )
                                    : null}
                                {hoverIdx != null && activeChartPoint ? (
                                    <HoverIndicator
                                        point={activeChartPoint}
                                        valueText={headerValue}
                                        timeText={activeTime}
                                        accentColor={accentColor}
                                        chartHeight={innerH}
                                        chartWidth={innerW}
                                    />
                                ) : null}
                            </g>
                        </svg>
                    </>
                ) : null}
            </div>

            {values.length > 0 ? (
                <div className="mt-1 flex justify-between gap-2 px-0.5 text-[10px] tabular-nums text-text-tertiary">
                    <span>{firstTime}</span>
                    <span>
                        {scaleMode === 'zero' ? '纵轴从 0' : '纵轴贴合'} · {lastTime}
                    </span>
                </div>
            ) : null}
        </div>
    );
}
