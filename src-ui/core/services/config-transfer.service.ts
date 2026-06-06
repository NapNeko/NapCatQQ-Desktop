// 配置导入导出 IPC 服务。

import { invoke, isTauri, pickDirectory, pickZipFile, saveZipFile } from '../ipc/transport';
import type {
    ConfigExportResult,
    ConfigImportPreview,
    ConfigImportResult,
} from '../ipc/types';

function defaultExportZipName(): string {
    return `napcat-config-export-${Math.floor(Date.now() / 1000)}.zip`;
}

export const configTransferService = {
    export: async (): Promise<ConfigExportResult | null> => {
        if (!isTauri) {
            return { export_path: '(浏览器预览不支持导出)', files: [] };
        }
        let dest = await saveZipFile('导出配置包', defaultExportZipName());
        if (!dest) return null;
        if (!dest.toLowerCase().endsWith('.zip')) {
            dest = `${dest}.zip`;
        }
        return invoke<ConfigExportResult>('export_config', { destPath: dest });
    },

    preview: async (sourcePath: string): Promise<ConfigImportPreview> => {
        if (!isTauri) {
            return {
                source_path: sourcePath,
                source_kind: 'directory',
                files_found: ['应用配置'],
                warnings: [],
                can_import: true,
            };
        }
        return invoke<ConfigImportPreview>('preview_config_import', { sourcePath });
    },

    import: async (sourcePath: string): Promise<ConfigImportResult> => {
        if (!isTauri) {
            return { files: [], skipped: [] };
        }
        return invoke<ConfigImportResult>('import_config', { sourcePath });
    },

    pickZipSource: async (): Promise<string | null> => pickZipFile('选择配置 ZIP 包'),

    pickDirectorySource: async (): Promise<string | null> =>
        pickDirectory('选择配置文件夹'),
};