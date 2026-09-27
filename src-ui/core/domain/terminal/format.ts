// 服务器状态条和文件栏的数字格式。

export function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes < 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
        value /= 1024;
        unit++;
    }
    const digits = unit === 0 || value >= 100 ? 0 : 1;
    return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatRate(bytesPerSec: number | undefined): string {
    if (bytesPerSec === undefined) return '—';
    return `${formatBytes(bytesPerSec)}/s`;
}

export function formatUptime(secs: number): string {
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const minutes = Math.floor((secs % 3600) / 60);
    if (days > 0) return `${days} 天 ${hours} 小时`;
    if (hours > 0) return `${hours} 小时 ${minutes} 分`;
    return `${minutes} 分`;
}

export function percent(used: number, total: number): number {
    if (!total) return 0;
    return Math.min(100, Math.max(0, (used / total) * 100));
}

/** 超过 90% 红，超过 75% 黄 */
export function loadTone(pct: number | undefined): 'normal' | 'warn' | 'danger' {
    if (pct === undefined) return 'normal';
    if (pct >= 90) return 'danger';
    if (pct >= 75) return 'warn';
    return 'normal';
}

/** 文件栏的修改时间：今天只写时分，今年写月日，更早写年月日 */
export function formatModified(unixSecs: number | undefined, now = new Date()): string {
    if (unixSecs === undefined) return '';
    const d = new Date(unixSecs * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    const sameDay =
        d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
    if (sameDay) return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
    if (d.getFullYear() === now.getFullYear()) return `${d.getMonth() + 1}-${pad(d.getDate())}`;
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
