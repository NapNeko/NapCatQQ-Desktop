// 应用端实例配置的 React 适配层：类型化配置读写 + 原始文件文档读写。
// 写成功后就地更新缓存并让实例列表失效（端口 / 对接可能变了）。错误不在这里弹 InfoBar：
// 冲突 / 校验要由页面分流（对话框 / 字段级提示），所以 mutation 只把 AppConfigError 抛回去。

import { useCallback, useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { APP_INSTANCES_KEY } from './appInstancesCache';
import type {
    AppConfigDocument,
    AppConfigError,
    AppConfigText,
    AppConfigWriteResult,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
    AppLinkBotDocument,
    AppPluginConfigSchema,
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
        queryFn: () =>
            appFrameworkService
                .readConfig(instanceId!)
                .catch((e) => Promise.reject(toAppConfigError(e))),
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

// 某个插件有哪些配置文件、能不能出表单。插件装卸、升级都会改这张表，所以每次打开都重列、关了就丢。
// schema 拿不到只是退回改原文，不算读取失败
export function useAppPluginConfigDocs(instanceId: string, pluginName: string | null) {
    return useQuery<{ docs: AppConfigDocument[]; schema: AppPluginConfigSchema | null }, Error>({
        queryKey: ['appPluginConfigDocs', instanceId, pluginName ?? ''],
        queryFn: async () => {
            const [docs, schema] = await Promise.all([
                appFrameworkService.listPluginConfigDocs(instanceId, pluginName!),
                appFrameworkService.pluginConfigSchema(instanceId, pluginName!).catch(() => null),
            ]);
            return { docs, schema };
        },
        enabled: !!pluginName,
        staleTime: 0,
        gcTime: 0,
        retry: false,
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

// Bot 侧（对接时桌面端写进协议 Bot 的连接配置）的只读文档列表，「原始文件」页单独一段展示。
export function useAppLinkBotDocs(instanceId: string) {
    return useQuery<AppLinkBotDocument[], Error>({
        queryKey: ['appLinkBotDocs', instanceId] as const,
        queryFn: () => appFrameworkService.linkBotDocuments(instanceId),
        retry: false,
    });
}
