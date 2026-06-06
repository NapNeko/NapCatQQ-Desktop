// 主页性能监控：采样间隔与历史窗口（对齐 legacy occupancy_card.py / OccupancyPanel）。

export const PERFORMANCE_MONITOR_INTERVAL_MS_MIN = 500;
export const PERFORMANCE_MONITOR_INTERVAL_MS_MAX = 10000;
export const PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT = 1200;
export const PERFORMANCE_MONITOR_HISTORY_SIZE = 24;

export function clampPerformanceMonitorIntervalMs(raw: number): number {
    if (!Number.isFinite(raw)) {
        return PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT;
    }
    return Math.max(
        PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
        Math.min(PERFORMANCE_MONITOR_INTERVAL_MS_MAX, Math.round(raw)),
    );
}

/** legacy: max(120, interval_ms - 20) */
export function performanceScrollDurationMs(sampleIntervalMs: number): number {
    const interval = clampPerformanceMonitorIntervalMs(sampleIntervalMs);
    return Math.max(120, interval - 20);
}