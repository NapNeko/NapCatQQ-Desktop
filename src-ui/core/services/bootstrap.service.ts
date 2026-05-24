// Bootstrap & 全局诊断 IPC 服务。
// 唯一持有这些 Tauri command 名字符串的位置（R3：单一字面量来源）。

import { invoke, isTauri } from '../ipc/transport';
import type { BootstrapSnapshot } from '../ipc/types';
import {
    mockBootstrap,
    mockDataDir,
    mockMigrationReportExport,
    withMockDelay,
} from '../ipc/mock/bootstrap.mock';

export const bootstrapService = {
    getStatus: async (): Promise<BootstrapSnapshot> => {
        if (isTauri) return invoke<BootstrapSnapshot>('get_bootstrap_status');
        return withMockDelay(mockBootstrap, 250);
    },

    openDataDir: async (): Promise<string> => {
        if (isTauri) return invoke<string>('open_data_dir');
        return mockDataDir;
    },

    exportMigrationReport: async (): Promise<string> => {
        if (isTauri) return invoke<string>('export_migration_report');
        return mockMigrationReportExport;
    },
};
