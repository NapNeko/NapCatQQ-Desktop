// Bot 实例运行时指标：间隔 / 保留天数 clamp（对齐 ncd-domain）

export const BOT_RUNTIME_METRICS_INTERVAL_MS_MIN = 1000;
export const BOT_RUNTIME_METRICS_INTERVAL_MS_MAX = 30_000;
export const BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT = 3000;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT = 7;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN = 1;
export const BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX = 90;

export function clampBotRuntimeMetricsIntervalMs(raw: number): number {
    if (!Number.isFinite(raw)) {
        return BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT;
    }
    return Math.max(
        BOT_RUNTIME_METRICS_INTERVAL_MS_MIN,
        Math.min(BOT_RUNTIME_METRICS_INTERVAL_MS_MAX, Math.round(raw)),
    );
}

export function clampBotRuntimeMetricsRetentionDays(raw: number): number {
    if (!Number.isFinite(raw)) {
        return BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT;
    }
    return Math.max(
        BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN,
        Math.min(BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX, Math.round(raw)),
    );
}

export function formatBytes(n: number | null | undefined): string {
    if (n == null || !Number.isFinite(n) || n < 0) return '—';
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
