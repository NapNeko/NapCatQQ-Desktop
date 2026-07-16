// 数据根整树迁移 IPC（命令名单一字面量来源）。

import { invoke, isTauri, listen, pickDirectory } from '../ipc/transport';
import type {
    DataRootMigratePreview,
    DataRootMigrateProgress,
    DataRootMigrateResult,
} from '../ipc/types';

export const DATA_ROOT_MIGRATE_PROGRESS_EVENT = 'data-root-migrate-progress';

export const dataRootMigrateService = {
    preview: async (targetRoot: string): Promise<DataRootMigratePreview> => {
        if (!isTauri) {
            return {
                source_root: 'C:\\ProgramData\\NapCatQQ Desktop',
                target_root: targetRoot,
                bytes_estimate: BigInt(12 * 1024 * 1024),
                local_active_bots: 0,
                tree_entries: [
                    { name: 'config/', kind: 'dir', bytes: BigInt(48 * 1024), note: null },
                    { name: 'components/', kind: 'dir', bytes: BigInt(10 * 1024 * 1024), note: null },
                    { name: 'secrets/', kind: 'dir', bytes: BigInt(4 * 1024), note: null },
                    { name: 'ssh_keys/', kind: 'dir', bytes: BigInt(2 * 1024), note: null },
                    { name: 'state/', kind: 'dir', bytes: BigInt(128 * 1024), note: null },
                    { name: 'logs/', kind: 'dir', bytes: BigInt(256 * 1024), note: null },
                    { name: 'layout-version.json', kind: 'file', bytes: BigInt(64), note: null },
                    { name: 'tmp', kind: 'skip', bytes: null, note: '不复制(可重建)' },
                ],
                blocking_reasons: [],
                warnings: ['浏览器预览：不会真实迁移', '迁移成功后需要重启'],
                ok: true,
            };
        }
        return invoke<DataRootMigratePreview>('preview_migrate_data_root', {
            targetRoot,
        });
    },

    start: async (targetRoot: string): Promise<DataRootMigrateResult> => {
        if (!isTauri) {
            return {
                old_root: 'C:\\ProgramData\\NapCatQQ Desktop',
                new_root: targetRoot,
                retired_marker_path: null,
                restart_required: true,
                warnings: ['浏览器预览：未真实迁移'],
            };
        }
        return invoke<DataRootMigrateResult>('start_migrate_data_root', {
            targetRoot,
        });
    },

    cancel: async (): Promise<void> => {
        if (!isTauri) return;
        await invoke<void>('cancel_migrate_data_root');
    },

    deleteRetired: async (oldRoot: string): Promise<void> => {
        if (!isTauri) return;
        await invoke<void>('delete_retired_data_root', { oldRoot });
    },

    pickTargetDirectory: async (): Promise<string | null> =>
        pickDirectory('选择新的数据根目录（须为空文件夹）'),

    listenProgress: async (
        handler: (progress: DataRootMigrateProgress) => void,
    ): Promise<() => void> => {
        if (!isTauri) {
            return () => undefined;
        }
        return listen<DataRootMigrateProgress>(DATA_ROOT_MIGRATE_PROGRESS_EVENT, handler);
    },
};
