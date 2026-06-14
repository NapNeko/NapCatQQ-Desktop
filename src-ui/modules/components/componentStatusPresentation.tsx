// 组件管理卡状态文案与 Badge 语义（MachineComponentRow / DockerRow 共用）。

import type { HostComponentStatus } from '../../core/domain/components/types';
import type { StatusBadgeSpec } from '../../core/domain/bot/bot-status-presentation';

export type { StatusBadgeSpec as StatusBadgeSpec };

export function hostComponentStatusBadge(
    status: HostComponentStatus,
    opts: { hasUpdate: boolean; inFlight: boolean },
): StatusBadgeSpec {
    if (opts.inFlight) {
        return { tone: 'brand', label: '进行中' };
    }
    switch (status.state) {
        case 'installed':
            if (opts.hasUpdate) return { tone: 'warning', label: '可更新' };
            return { tone: 'success', label: '已安装', dot: true };
        case 'not_installed':
            return { tone: 'neutral', label: '未安装', dot: true };
        case 'unsupported':
            return { tone: 'neutral', label: '不支持' };
        case 'unknown':
            if (status.reason === '正在探测') {
                return { tone: 'warning', label: '探测中' };
            }
            return { tone: 'danger', label: '探测失败' };
    }
}

export function dockerRowStatusBadge(opts: {
    ready: boolean;
    probing: boolean;
    inFlight: boolean;
}): StatusBadgeSpec {
    if (opts.inFlight) return { tone: 'brand', label: '安装中' };
    if (opts.probing) return { tone: 'warning', label: '探测中' };
    if (opts.ready) return { tone: 'success', label: '已就绪', dot: true };
    return { tone: 'neutral', label: '未安装', dot: true };
}