// 应用端实例配置的 React 适配层：类型化配置读写 + 原始文件文档读写。
// 写成功后就地更新缓存并让实例列表失效（端口 / 对接可能变了）。错误不在这里弹 InfoBar：
// 冲突 / 校验要由页面分流（对话框 / 字段级提示），所以 mutation 只把 AppConfigError 抛回去。

import { useCallback, useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { APP_INSTANCES_KEY } from './useAppInstances';
import type {
    AppConfigDocument,
    AppConfigError,
    AppConfigText,
    AppConfigWriteResult,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
} from '../../core/ipc/types';

export const appConfigKey = (instanceId: string) => ['appInstanceConfig', instanceId] as const;
export const appConfigDocsKey = (instanceId: string) => ['appConfigDocs', instanceId] as const;
export const appConfigTextKey = (instanceId: string, docId: string) =>
    ['appConfigText', instanceId, docId] as const;

export interface WriteConfigArgs {
    config: AppInstanceConfig;
    /** null = 覆盖（用户在冲突对话框选了「覆盖」） */
    baseRevision: string | null;
    confId?: string | null;
}

export function useAppInstanceConfig(instanceId: string | null, enabled = true) {
    const queryClient = useQueryClient();
    const key = appConfigKey(instanceId ?? '');

    const query = useQuery<AppInstanceConfigEnvelope, AppConfigError>({
        queryKey: key,
        queryFn: () => appFrameworkService.readConfig(instanceId!).catch((e) => Promise.reject(toAppConfigError(e))),
        enabled: enabled && !!instanceId,
        retry: false,
        staleTime: 15_000,
    });

    const writeMutation = useMutation<AppConfigWriteResult, AppConfigError, WriteConfigArgs>({
        mutationFn: ({ config, baseRevision, confId }) =>
            appFrameworkService
                .writeConfig(instanceId!, config, baseRevision, confId)
                .catch((e) => Promise.reject(toAppConfigError(e))),
        onSuccess: (result) => {
            queryClient.setQueryData<AppInstanceConfigEnvelope>(key, {
                config: result.config,
                revision: result.revision,
                documents: result.documents,
            });
            queryClient.invalidateQueries({ queryKey: APP_INSTANCES_KEY });
            if (result.relinked) queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            // 原始文件文本随之变化
            queryClient.invalidateQueries({ queryKey: ['appConfigText', instanceId] });
        },
    });

    const refetch = query.refetch;
    const reload = useCallback(() => refetch(), [refetch]);
    const write = writeMutation.mutateAsync;

    return useMemo(
        () => ({
            envelope: query.data ?? null,
            isLoading: query.isLoading,
            error: query.error ?? null,
            reload,
            write,
            isWriting: writeMutation.isPending,
        }),
        [query.data, query.error, query.isLoading, reload, write, writeMutation.isPending],
    );
}

export function useAppConfigDocuments(instanceId: string | null) {
    return useQuery<AppConfigDocument[], Error>({
        queryKey: appConfigDocsKey(instanceId ?? ''),
        queryFn: () => appFrameworkService.listConfigDocuments(instanceId!),
        enabled: !!instanceId,
        staleTime: Infinity,
    });
}

export interface WriteTextArgs {
    text: string;
    baseRevision: string | null;
}

export function useAppConfigText(instanceId: string | null, docId: string | null) {
    const queryClient = useQueryClient();
    const key = appConfigTextKey(instanceId ?? '', docId ?? '');

    const query = useQuery<AppConfigText, AppConfigError>({
        queryKey: key,
        queryFn: () =>
            appFrameworkService
                .readConfigText(instanceId!, docId!)
                .catch((e) => Promise.reject(toAppConfigError(e))),
        enabled: !!instanceId && !!docId,
        retry: false,
        staleTime: 15_000,
    });

    const writeMutation = useMutation<AppConfigText, AppConfigError, WriteTextArgs>({
        mutationFn: ({ text, baseRevision }) =>
            appFrameworkService
                .writeConfigText(instanceId!, docId!, text, baseRevision)
                .catch((e) => Promise.reject(toAppConfigError(e))),
        onSuccess: (saved) => {
            queryClient.setQueryData<AppConfigText>(key, saved);
            // 手改原文可能动到端口 / 对接键，类型化视图与实例列表都要重新拉
            queryClient.invalidateQueries({ queryKey: appConfigKey(instanceId ?? '') });
            queryClient.invalidateQueries({ queryKey: APP_INSTANCES_KEY });
        },
    });

    return {
        doc: query.data ?? null,
        isLoading: query.isLoading,
        error: query.error ?? null,
        reload: query.refetch,
        write: writeMutation.mutateAsync,
        isWriting: writeMutation.isPending,
    };
}
