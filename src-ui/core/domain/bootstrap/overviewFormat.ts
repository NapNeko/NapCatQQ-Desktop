// 概览页各卡的小格式规则：通知行的相对时间、Core 卡版本号补 v、远端主机显示名。
// 通知那套「刚刚 / N 分钟前」和 core/domain/ui/relativeTime.ts 阈值不同（输入是秒、15 分钟内标红、
// 当天改显「今天 HH:MM」），不是同一件事，别合并。

import type { ServerProfile } from '../../ipc/generated/domain/ServerProfile';

export function formatRelativeNoticeTime(unixSeconds: number): { text: string; isRecent: boolean } {
    const nowSec = Math.floor(Date.now() / 1000);
    const diffSec = Math.max(0, nowSec - unixSeconds);

    if (diffSec < 60) {
        return { text: '刚刚', isRecent: true };
    }
    if (diffSec < 3600) {
        const mins = Math.floor(diffSec / 60);
        return { text: `${mins} 分钟前`, isRecent: mins < 15 };
    }
    if (diffSec < 86400) {
        const d = new Date(unixSeconds * 1000);
        const hh = String(d.getHours()).padStart(2, '0');
        const mm = String(d.getMinutes()).padStart(2, '0');
        return { text: `今天 ${hh}:${mm}`, isRecent: false };
    }
    const d = new Date(unixSeconds * 1000);
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return { text: `${mm}-${dd}`, isRecent: false };
}

export function formatVersion(raw: string): string {
    return /^[vV]/.test(raw) ? raw : `v${raw}`;
}

export function serverDisplayName(p: ServerProfile): string {
    return p.name?.trim() || p.host?.trim() || p.id;
}
