// 浏览器预览模式下的 Bootstrap 假数据库。
// 真 IPC 实装在 `core/services/bootstrap.service.ts`。

import type { BootstrapSnapshot } from '../types';

export const mockBootstrap: BootstrapSnapshot = {
    status: 'ready',
    schema_version: 'v3',
    report: {
        stage: 'completed',
        outcome: 'updated',
        warnings: [
            { code: 'W001', message: '发现旧版配置文件残留，已自动进行合并。' },
        ],
        source: {
            path: 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Legacy',
            detected_version: 'v2.1.0',
        },
        backup: {
            backup_dir: 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\backup_v2_v3',
            timestamp: Date.now() - 3600000,
        },
        rules_applied: [
            'MigrateLocalAccounts',
            'NormalizePortBindings',
            'CleanLegacyTempCache',
        ],
        repair_actions: ['open_data_dir', 'export_migration_report'],
    },
    data_root: 'C:\\ProgramData\\NapCatQQ Desktop',
    local_versions: {
        napcat: '4.18.1',
        snowluma: null,
    },
};

export const mockDataDir = 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\data';
export const mockMigrationReportExport =
    'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\exports\\migration-report-1234567.json';

/// 模拟 IPC 延迟。
export function withMockDelay<T>(value: T, ms = 250): Promise<T> {
    return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}
