// 麦麦提示词：目录、单个模板、版本内容、保存等操作。跑着走 WebUI、停着改盘由后端按实例状态选；
// 这里的 mode 只决定查不查、缓存按哪份算：刚启动 WebUI 还没起来时先不查，免得报一串连不上。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { maibotResourcesService as svc } from '../../core/services/maibot-resources.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile } from '../../core/ipc/types';

/** disk：停着改盘；live：跑着且 WebUI 应答了；waiting：跑着但 WebUI 还没起来 */
export type PromptMode = 'disk' | 'live' | 'waiting';

export const maibotPromptCatalogKey = (id: string, mode: PromptMode) => ['maibotPromptCatalog', id, mode] as const;
export const maibotPromptFileKey = (id: string, mode: PromptMode, language: string, name: string) =>
    ['maibotPromptFile', id, mode, language, name] as const;

export function useMaiBotPromptCatalog(instanceId: string, mode: PromptMode) {
    return useQuery<MaiBotPromptCatalog, Error>({
        queryKey: maibotPromptCatalogKey(instanceId, mode),
        queryFn: () => svc.promptCatalog(instanceId),
        enabled: mode !== 'waiting',
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotPromptFile(instanceId: string, mode: PromptMode, language?: string, name?: string) {
    return useQuery<MaiBotPromptFile, Error>({
        queryKey: maibotPromptFileKey(instanceId, mode, language ?? '', name ?? ''),
        queryFn: () => svc.promptFile(instanceId, language!, name!),
        enabled: mode !== 'waiting' && !!language && !!name,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotPromptVersion(
    instanceId: string,
    mode: PromptMode,
    language: string,
    name: string,
    versionId: string | null,
) {
    return useQuery<string, Error>({
        queryKey: ['maibotPromptVersion', instanceId, mode, language, name, versionId],
        queryFn: () => svc.promptVersion(instanceId, language, name, versionId!),
        enabled: mode !== 'waiting' && !!versionId,
        retry: false,
        staleTime: 60_000,
    });
}

export function useMaiBotPromptAction(instanceId: string, mode: PromptMode) {
    const qc = useQueryClient();
    return useMutation<MaiBotPromptFile, unknown, MaiBotPromptAction>({
        mutationFn: (action) => svc.promptAction(instanceId, action),
        onSuccess: (file) => {
            qc.setQueryData(maibotPromptFileKey(instanceId, mode, file.language, file.name), file);
            void qc.invalidateQueries({ queryKey: ['maibotPromptCatalog', instanceId] });
        },
        onError: (err) => {
            pushErrorBar({
                key: `maibot-prompt:${instanceId}`,
                title: '提示词没存上',
                raw: toAppConfigError(err).message,
            });
        },
    });
}
