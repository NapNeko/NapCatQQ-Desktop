import { useCallback, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { errorText } from '../../core/domain/errors';
import {
    APP_STORE_GC_MS,
    APP_STORE_STALE_MS,
    appStoreInstalledKey,
    appStoreMarketKey,
} from './appStoreQuery';
import { reportCatalogError, usePluginOps } from './usePluginOps';
import {
    filterAppStore,
    overlayStoreInstalled,
    storeOpErrorCopy,
    type StoreKindFilter,
} from '../../core/domain/apps/appStore';
import type {
    AppInstance,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
} from '../../core/ipc/types';

/** 应用端商店页（NoneBot2 的适配器 / 插件，AstrBot、MaiBot 的插件）；插件页顺带读已装适配器，按它筛插件 */
export function useAppStore(instance: AppInstance, resource: AppStoreResource) {
    const queryClient = useQueryClient();

    const marketKey = appStoreMarketKey(instance.framework_id, resource);
    const installedKey = appStoreInstalledKey(instance.id, resource);
    const adaptersKey = appStoreInstalledKey(instance.id, 'adapter');

    const marketQ = useQuery({
        queryKey: marketKey,
        queryFn: async (): Promise<AppStoreMarketEntry[]> => {
            try {
                return await appFrameworkService.listStore(instance.framework_id, resource);
            } catch (e) {
                reportCatalogError(`app-store-market:${instance.framework_id}:${resource}`, e);
            }
        },
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });

    const installedQ = useQuery({
        queryKey: installedKey,
        queryFn: async (): Promise<AppStoreInstalled[]> => {
            try {
                return await appFrameworkService.listStoreInstalled(instance.id, resource);
            } catch (e) {
                reportCatalogError(`app-store-installed:${instance.id}:${resource}`, e);
            }
        },
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });

    const adaptersQ = useQuery({
        queryKey: adaptersKey,
        queryFn: async (): Promise<AppStoreInstalled[]> => {
            try {
                return await appFrameworkService.listStoreInstalled(instance.id, 'adapter');
            } catch (e) {
                reportCatalogError(`app-store-installed:${instance.id}:adapter`, e);
            }
        },
        enabled: resource === 'plugin',
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });

    const market = marketQ.data ?? [];
    const installed = installedQ.data ?? [];
    const adapters = adaptersQ.data ?? [];
    const catalogError = marketQ.error ?? installedQ.error ?? (resource === 'plugin' ? adaptersQ.error : null);
    const loading =
        marketQ.isFetching
        || installedQ.isFetching
        || (resource === 'plugin' && adaptersQ.isFetching);

    const [query, setQuery] = useState('');
    const [kindFilter, setKindFilter] = useState<StoreKindFilter>('all');

    const reload = useCallback(async () => {
        await Promise.all([
            queryClient.refetchQueries({ queryKey: marketKey }),
            queryClient.refetchQueries({ queryKey: installedKey }),
            resource === 'plugin'
                ? queryClient.refetchQueries({ queryKey: adaptersKey })
                : Promise.resolve(),
        ]);
    }, [adaptersKey, installedKey, marketKey, queryClient, resource]);

    const reloadInstalled = useCallback(async () => {
        await queryClient.invalidateQueries({ queryKey: installedKey });
        if (resource === 'plugin') {
            await queryClient.invalidateQueries({ queryKey: adaptersKey });
        }
    }, [adaptersKey, installedKey, queryClient, resource]);

    const { taskHints, ...ops } = usePluginOps(instance, {
        resource,
        barKey: 'app-store',
        runningHint: '需重启后生效',
        failNoun: resource === 'adapter' ? '适配器' : '插件',
        submitFailTitle: '操作失败',
        errorCopy: storeOpErrorCopy,
        reloadInstalled,
    });

    const enabledAdapterModules = useMemo(
        () =>
            (resource === 'plugin' ? adapters : installed)
                .filter((item) => item.enabled)
                .map((item) => item.id),
        [adapters, installed, resource],
    );

    const rows = useMemo(
        () =>
            filterAppStore({
                resource,
                entries: market,
                installed: overlayStoreInstalled(installed, taskHints, resource),
                query,
                kindFilter,
                enabledAdapterModules,
                linked: !!instance.link,
            }),
        [enabledAdapterModules, installed, instance.link, kindFilter, market, query, resource, taskHints],
    );

    return {
        rows,
        query,
        setQuery,
        kindFilter,
        setKindFilter,
        loading,
        error: catalogError ? errorText(catalogError) : null,
        reload,
        ...ops,
    };
}
