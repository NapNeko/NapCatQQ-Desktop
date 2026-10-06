// 「3 分钟前」式的相对时间，Bot 卡的状态时间和插件商店的上架时间共用一套阈值。
// 运行指标的采集时间、麦麦资源页、概览通知的写法不一样（输入单位不同，多久以后改显示日期也不同），各自留着。

/** 毫秒时间戳换成相对时间。给了 maxDays 就只说这么多天以内的，更早的返回 null，调用方干脆不显示 */
export function relativeTimeFromMs(
    ms: number,
    maxDays?: number,
    nowMs = Date.now(),
): string | null {
    const diffSec = Math.max(0, Math.floor((nowMs - ms) / 1000));
    if (diffSec < 5) return '刚刚';
    if (diffSec < 60) return `${diffSec} 秒前`;
    const min = Math.floor(diffSec / 60);
    if (min < 60) return `${min} 分钟前`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr} 小时前`;
    const day = Math.floor(hr / 24);
    if (maxDays !== undefined && day >= maxDays) return null;
    if (day < 7) return `${day} 天前`;
    const week = Math.floor(day / 7);
    if (week < 4) return `${week} 周前`;
    // 28、29 天按周算已满 4 周、按月算还不到 1 个月，360 多天按月满 12、按年不到 1 年；这两段往上取到 1
    const month = Math.max(1, Math.floor(day / 30));
    if (month < 12) return `${month} 个月前`;
    const year = Math.max(1, Math.floor(day / 365));
    return `${year} 年前`;
}

/** ISO 时间串换成相对时间；解析不了返回 null */
export function formatRelativeTime(iso: string): string | null {
    const ts = Date.parse(iso);
    if (Number.isNaN(ts)) return null;
    return relativeTimeFromMs(ts);
}
