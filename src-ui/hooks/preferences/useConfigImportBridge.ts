import { useEffect } from 'react';
import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { configTransferService } from '../../core/services/config-transfer.service';
import type { ConfigImportResult } from '../../core/ipc/types';
import { hydrateAppUiPreferencesFromDisk } from './useAppUiPreferencesBootstrap';

const affectedQueries = new Set(['appSettings', 'botConfig', 'botSnapshots', 'botFlavors', 'servers', 'appInstances', 'debug', 'configTransfer',
    'appInstanceConfig', 'appConfigDocs', 'appConfigText', 'appPluginConfigDocs', 'appStoreInstalled', 'karinPluginsInstalled', 'appLinkPlan', 'appWebUiAccount',
    'maibotPromptCatalog', 'maibotPromptFile', 'maibotPromptVersion']);

export async function refreshImportedConfiguration(client: QueryClient, files: ConfigImportResult['files']): Promise<void> {
    if (files.includes('API 调试工作区')) {
        const { debugWorkspaceStore, markWorkspaceStale } = await import('../debug/debugWorkspaceStore');
        markWorkspaceStale();
        await debugWorkspaceStore.load();
    }
    if (files.includes('应用设置') || files.includes('离线通知设置(app-settings)')) {
        await hydrateAppUiPreferencesFromDisk(true);
    }
    await client.invalidateQueries({ predicate: query => affectedQueries.has(String(query.queryKey[0])) });
}

/** 所有窗口在配置导入后重新读取，避免弹出窗将旧工作区再次写回。 */
export function useConfigImportBridge(): void {
    const client = useQueryClient();
    useEffect(() => {
        let active = true;
        let stop: (() => void) | undefined;
        void configTransferService.onImported(files => {
            if (active) void refreshImportedConfiguration(client, files);
        }).then(unlisten => {
            if (active) stop = unlisten;
            else unlisten();
        }).catch(error => {
            console.warn('配置导入通知订阅失败:', error);
        });
        return () => { active = false; stop?.(); };
    }, [client]);
}
