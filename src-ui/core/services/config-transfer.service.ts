// 配置导入导出 IPC 服务。
// 唯一持有 export_config / import_config 命令名 + 目录选择的位置（R3）。
//
// 流程：先弹原生目录对话框拿路径，再把路径交给后端 command 做实际复制 / 校验写回。
// 用户取消对话框时返回 null，hook 据此静默收场（不当错误）。

import { invoke, isTauri, pickDirectory } from '../ipc/transport';
import type { ConfigExportResult, ConfigImportResult } from '../ipc/types';

export const configTransferService = {
    /// 弹目录选择 → 导出到该目录。用户取消返回 null。
    export: async (): Promise<ConfigExportResult | null> => {
        if (!isTauri) {
            return { export_dir: '(浏览器预览不支持导出)', files: [] };
        }
        const dir = await pickDirectory('选择导出目录');
        if (!dir) return null;
        return invoke<ConfigExportResult>('export_config', { destDir: dir });
    },

    /// 弹目录选择 → 从该目录导入。用户取消返回 null。
    import: async (): Promise<ConfigImportResult | null> => {
        if (!isTauri) {
            return { files: [], skipped: [] };
        }
        const dir = await pickDirectory('选择配置来源目录');
        if (!dir) return null;
        return invoke<ConfigImportResult>('import_config', { srcDir: dir });
    },
};
