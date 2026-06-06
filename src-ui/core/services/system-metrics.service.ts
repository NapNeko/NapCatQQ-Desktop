// 本机资源占用 IPC。命令名仅在此 service 出现。

import { invoke, isTauri } from '../ipc/transport';
import type { SystemResourceSnapshot } from '../ipc/generated/SystemResourceSnapshot';

export type SystemMetricsSnapshotOptions = {
    /** 首屏进入概览：后端跳过 CPU 最小间隔等待，尽快返回一次读数。 */
    bootstrap?: boolean;
};

export const systemMetricsService = {
    snapshot: async (
        options: SystemMetricsSnapshotOptions = {},
    ): Promise<SystemResourceSnapshot> => {
        if (!isTauri) {
            return { cpuPercent: 0, ramPercent: 0 };
        }
        return invoke<SystemResourceSnapshot>('get_system_resource_snapshot', {
            bootstrap: options.bootstrap ?? false,
        });
    },
};