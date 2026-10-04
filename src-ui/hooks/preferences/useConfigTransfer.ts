// 配置导入导出 hook。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, useSyncExternalStore } from 'react';
import { configTransferService, type ConfigImportOutcome } from '../../core/services/config-transfer.service';
import type { ConfigFrontendPreferences } from '../../core/ipc/types';
import { flushWorkspace } from '../debug/debugWorkspaceStore';
import { refreshImportedConfiguration } from './useConfigImportBridge';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { createStore } from '../utils/createStore';

const pendingPreferencesStore = createStore<ConfigFrontendPreferences | null>(null);
const frameworkPendingKey = ['configTransfer', 'frameworkPending'] as const;

export type { ConfigImportOutcome } from '../../core/services/config-transfer.service';

/** 导入向导里的三步：挑 ZIP、预览、真导入。结果只给向导自己用，失败由向导按阶段显示 */
export function useConfigImportOps() {
    const client = useQueryClient();
    const preview = useMutation({ mutationFn: configTransferService.preview });
    const runImport = useMutation({
        mutationFn: async (sourcePath: string) => {
            await flushWorkspace();
            const result = await configTransferService.import(sourcePath);
            await refreshImportedConfiguration(client, result.files);
            return result;
        },
    });
    return {
        pickZipSource: configTransferService.pickZipSource,
        previewImport: preview.mutateAsync,
        importConfig: runImport.mutateAsync,
    };
}

export function useConfigTransfer() {
    const client = useQueryClient();
    const [importOpen, setImportOpen] = useState(false);
    const pendingPreferences = useSyncExternalStore(pendingPreferencesStore.subscribe, pendingPreferencesStore.getSnapshot, pendingPreferencesStore.getSnapshot);
    const pendingFrameworks = useQuery({ queryKey: frameworkPendingKey, queryFn: configTransferService.pendingFrameworkConfigs });
    const retryFrameworks = useMutation({
        mutationFn: configTransferService.retryFrameworkConfigs,
        onSuccess: async result => {
            const pending = result.framework_pending ?? [];
            client.setQueryData(frameworkPendingKey, pending);
            await refreshImportedConfiguration(client, result.files);
            pushInfoBar({ key: 'config-import-frameworks', tone: pending.length ? 'warning' : 'success',
                title: pending.length ? '部分框架配置仍待恢复' : '框架配置已恢复',
                content: pending.length ? pending.join('；') : '启动实例后会使用恢复的配置。' });
        },
        onError: (error: Error) => pushErrorBar({ key: 'config-import-frameworks', title: '框架配置恢复失败', raw: error.message || String(error) }),
    });

    const exportMutation = useMutation({
        mutationFn: async () => {
            await flushWorkspace();
            return configTransferService.export();
        },
        onSuccess: (result) => {
            if (!result) return;
            const files = result.files.length ? result.files.join('、') : '无可导出项';
            pushInfoBar({
                key: 'config-export',
                tone: 'success',
                title: '配置已导出',
                content: `${files} → ${result.export_path}`,
            });
        },
        onError: (err: Error) => {
            pushErrorBar({
                key: 'config-export',
                title: '导出失败',
                raw: err.message || String(err),
            });
        },
    });

    const handleImported = (result: ConfigImportOutcome) => {
        const frameworkPending = result.framework_pending ?? [];
        if (frameworkPending.length) client.setQueryData(frameworkPendingKey, frameworkPending);
        else void client.invalidateQueries({ queryKey: frameworkPendingKey });
        const skippedNote = result.skipped.length
            ? `；未覆盖：${result.skipped.join('、')}`
            : '';
        pendingPreferencesStore.setState(result.frontendPreferencesError ? result.frontend_preferences ?? null : null);
        const incomplete = Boolean(result.frontendPreferencesError) || frameworkPending.length > 0;
        const pendingNotes = [
            result.frontendPreferencesError ? `界面偏好：${result.frontendPreferencesError}` : '',
            ...frameworkPending,
        ].filter(Boolean).join('；');
        pushInfoBar({
            key: 'config-import',
            tone: incomplete ? 'warning' : 'success',
            title: incomplete ? '配置已导入，部分配置待恢复' : '配置已导入，重启后生效',
            content: `已导入：${result.files.join('、')}${skippedNote}。${incomplete
                ? `${pendingNotes}，可在配置备份中重试恢复。`
                : 'SSH 密码、私钥和系统密钥库中的凭据需另外配置。'}`,
        });
        setImportOpen(false);
    };

    const retryPreferences = () => {
        if (!pendingPreferences) return;
        try {
            configTransferService.restorePreferences(pendingPreferences);
            pendingPreferencesStore.setState(null);
            pushInfoBar({ key: 'config-import', tone: 'success', title: '界面与终端偏好已恢复', content: pendingFrameworks.data?.length
                ? '框架配置仍待恢复，可继续单独重试。' : '配置导入完成，启动相关设置在重启后生效。' });
        } catch (error) {
            pushErrorBar({ key: 'config-import-preferences', title: '界面偏好恢复失败', raw: error instanceof Error ? error.message : String(error) });
        }
    };

    return {
        exportConfig: () => exportMutation.mutate(),
        openImportWizard: () => setImportOpen(true),
        importOpen,
        setImportOpen,
        onImported: handleImported,
        canRetryPreferences: pendingPreferences !== null,
        retryPreferences,
        pendingFrameworks: pendingFrameworks.data ?? [],
        frameworkPendingError: pendingFrameworks.error,
        retryFrameworks: () => retryFrameworks.mutate(),
        isRestoringFrameworks: retryFrameworks.isPending,
        isExporting: exportMutation.isPending,
        isImporting: false,
    };
}
