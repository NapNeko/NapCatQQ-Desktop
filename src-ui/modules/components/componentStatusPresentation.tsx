// 组件管理卡状态文案与 Badge 语义（MachineComponentRow / DockerRow 共用）。

import type { HostComponentStatus } from '../../core/domain/components/types';
import type { StatusBadgeSpec } from '../../core/domain/bot/bot-status-presentation';
import type { UninstallSupport } from '../../core/ipc/types';

export type { StatusBadgeSpec as StatusBadgeSpec };

export function isExternalNodeSource(source: string): boolean {
    return source.trim().toLowerCase().startsWith('$path');
}

export function shouldOfferManagedNodeInstall(status: HostComponentStatus): boolean {
    return !(status.state === 'installed' && isExternalNodeSource(status.detected.source));
}

/// 卸载入口的动作形态：跑组件任务 / 跳系统卸载流 / 不渲染
export type UninstallEntryAction = 'task' | 'system' | 'none';

/// 行上「卸载」应该长成什么样。未安装与 $PATH 外部探测来源（不归桌面端管）
/// 一律没有卸载入口，与组件声明的能力无关
export function uninstallEntryAction(
    uninstall: UninstallSupport,
    status: HostComponentStatus,
): UninstallEntryAction {
    if (status.state !== 'installed') return 'none';
    if (isExternalNodeSource(status.detected.source)) return 'none';
    switch (uninstall) {
        case 'supported':
            return 'task';
        case 'system_managed':
            return 'system';
        case 'not_supported':
            return 'none';
    }
}

export function hostComponentStatusBadge(
    status: HostComponentStatus,
    opts: { hasUpdate: boolean; inFlight: boolean },
): StatusBadgeSpec {
    if (opts.inFlight) {
        return { tone: 'brand', label: '进行中' };
    }
    switch (status.state) {
        case 'installed':
            if (status.detected.source && isExternalNodeSource(status.detected.source)) {
                return { tone: 'neutral', label: '系统环境', dot: true };
            }
            if (opts.hasUpdate) return { tone: 'warning', label: '可更新' };
            return { tone: 'success', label: '已安装', dot: true };
        case 'not_installed':
            return { tone: 'neutral', label: '未安装', dot: true };
        case 'unusable':
            return {
                tone: 'warning',
                label: status.unusable.version ? '版本不符' : '无法运行',
                dot: true,
            };
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
