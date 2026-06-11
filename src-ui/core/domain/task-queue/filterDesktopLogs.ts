// 任务队列：按 startedAt 过滤 Desktop tail 行（纯函数）。

import { parseDesktopLogLine } from '../events/log-buffer';

const DESKTOP_TIME_RE = /^(\d{2})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})/;

function desktopPreviewToMs(timestamp: string): number | null {
    const m = timestamp.trim().match(DESKTOP_TIME_RE);
    if (!m) return null;
    const year = 2000 + Number(m[1]);
    const month = Number(m[2]) - 1;
    const day = Number(m[3]);
    const hour = Number(m[4]);
    const min = Number(m[5]);
    const sec = Number(m[6]);
    const d = new Date(year, month, day, hour, min, sec);
    const t = d.getTime();
    return Number.isNaN(t) ? null : t;
}

export function filterDesktopLogLinesSince(lines: string[], startedAtMs: number): string[] {
    if (startedAtMs <= 0) return lines;
    return lines.filter((line) => {
        const parsed = parseDesktopLogLine(line);
        const t = desktopPreviewToMs(parsed.timestamp);
        if (t == null) return true;
        return t >= startedAtMs - 2000;
    });
}