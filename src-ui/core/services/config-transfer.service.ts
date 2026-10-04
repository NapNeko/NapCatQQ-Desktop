// 配置导入导出 IPC 服务。

import { invoke, isTauri, listen, pickDirectory, pickZipFile, saveZipFile } from '../ipc/transport';
import type {
    ConfigExportResult,
    ConfigFrontendPreferences,
    ConfigImportPreview,
    ConfigImportResult,
} from '../ipc/types';
import {
    collectFrontendPreferences,
    restoreFrontendPreferences,
} from '../domain/settings/config-transfer-preferences';

export type ConfigImportOutcome = ConfigImportResult & { frontendPreferencesError?: string };

function defaultExportZipName(): string {
    return `napcat-config-export-${Math.floor(Date.now() / 1000)}.zip`;
}

export const configTransferService = {
    export: async (): Promise<ConfigExportResult | null> => {
        if (!isTauri) {
            throw new Error('请在桌面应用中导出配置备份');
        }
        let dest = await saveZipFile('导出配置包', defaultExportZipName());
        if (!dest) return null;
        if (!dest.toLowerCase().endsWith('.zip')) {
            dest = `${dest}.zip`;
        }
        const frontendPreferences = collectFrontendPreferences();
        return invoke<ConfigExportResult>('export_config', { destPath: dest, frontendPreferences });
    },

    preview: async (sourcePath: string): Promise<ConfigImportPreview> => {
        if (!isTauri) {
            return {
                source_path: sourcePath,
                source_kind: 'directory',
                files_found: ['应用设置', '应用实例与关联', '聊天账号偏好', 'API 调试工作区', 'API 调试收藏', '界面与终端偏好'],
                warnings: [],
                can_import: true,
            };
        }
        return invoke<ConfigImportPreview>('preview_config_import', { sourcePath });
    },

    import: async (sourcePath: string): Promise<ConfigImportOutcome> => {
        if (!isTauri) {
            return { files: [], skipped: [] };
        }
        const result = await invoke<ConfigImportResult>('import_config', { sourcePath });
        if (result.frontend_preferences) {
            try { restoreFrontendPreferences(result.frontend_preferences); }
            catch (error) {
                return { ...result, frontendPreferencesError: error instanceof Error ? error.message : String(error) };
            }
        }
        return result;
    },

    restorePreferences: (snapshot: ConfigFrontendPreferences): void => restoreFrontendPreferences(snapshot),

    pendingFrameworkConfigs: async (): Promise<string[]> => isTauri
        ? invoke<string[]>('list_pending_framework_config_restores') : [],

    retryFrameworkConfigs: async (): Promise<ConfigImportResult> => isTauri
        ? invoke<ConfigImportResult>('retry_framework_config_restore') : { files: [], skipped: [], framework_pending: [] },

    onImported: (callback: (files: ConfigImportResult['files']) => void): Promise<() => void> => {
        if (!isTauri) return Promise.resolve(() => {});
        return listen<ConfigImportResult['files']>('config-imported', callback);
    },

    pickZipSource: async (): Promise<string | null> => pickZipFile('选择配置 ZIP 包'),

    pickDirectorySource: async (): Promise<string | null> =>
        pickDirectory('选择配置文件夹'),
};
