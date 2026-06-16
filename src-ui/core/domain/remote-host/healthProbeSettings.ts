// 远程主机健康探活：后台低频探测已连接远端主机的连通性。
// 对齐 Rust 侧 AppSettings 的 remote_host_health_probe_interval_ms（clamp 10s~5min）。

export const REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN = 10_000;
export const REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX = 300_000;
export const REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT = 30_000;

export function clampRemoteHostHealthProbeIntervalMs(raw: number): number {
    if (!Number.isFinite(raw)) {
        return REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT;
    }
    return Math.max(
        REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN,
        Math.min(REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX, Math.round(raw)),
    );
}
