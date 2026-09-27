import { useCallback, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { errorText } from '../../core/domain/errors';
import {
    APP_STORE_GC_MS,
    APP_STORE_STALE_MS,
    karinPluginMarketKey,
    karinPluginsInstalledKey,
} from './appStoreQuery';
import { reportCatalogError, usePluginOps } from './usePluginOps';
import {
    KARIN_CATALOG,
    filterKarinPlugins,
    overlayKarinInstalled,
    type KarinPluginKindFilter,
} from '../../core/domain/apps/karinPlugins';
import type { AppInstance, KarinPluginInstalled, KarinPluginMarketEntry } from '../../core/ipc/types';

export function useKarinPlugins(instance: AppInstance) {
    const queryClient = useQueryClient();
    const installedKey = karinPluginsInstalledKey(instance.id);

    const marketQ = useQuery({
        queryKey: karinPluginMarketKey,
        queryFn: async (): Promise<KarinPluginMarketEntry[]> => {
            try {
                return await appFrameworkService.listPluginMarket();
            } catch (e) {
                reportCatalogError('karin-plugin-market', e, KARIN_CATALOG);
            }
        },
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });

    const installedQ = useQuery({
        queryKey: installedKey,
        queryFn: async (): Promise<KarinPluginInstalled[]> => {
            try {
                return await appFrameworkService.listPlugins(instance.id);
            } catch (e) {
                reportCatalogError(`karin-plugin-installed:${instance.id}`, e, KARIN_CATALOG);
            }
        },
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });

    const market = marketQ.data ?? [];
    const installed = installedQ.data ?? [];
    const catalogError = marketQ.error ?? installedQ.error;
    const loading = marketQ.isFetching || installedQ.isFetching;

    const [query, setQuery] = useState('');
    const [kindFilter, setKindFilter] = useState<KarinPluginKindFilter>('all');

    const reload = useCallback(async () => {
        await Promise.all([
            queryClient.refetchQueries({ queryKey: karinPluginMarketKey }),
            queryClient.refetchQueries({ queryKey: installedKey }),
        ]);
    }, [installedKey, queryClient]);

    const reloadInstalled = useCallback(async () => {
        await queryClient.invalidateQueries({ queryKey: installedKey });
    }, [installedKey, queryClient]);

    const { taskHints, ...ops } = usePluginOps(instance, {
        barKey: 'karin-plugin',
        runningHint: 'Karin 会热加载',
        failNoun: '',
        submitFailTitle: '插件操作失败',
        reloadInstalled,
    });

    const rows = useMemo(
        () => filterKarinPlugins(market, overlayKarinInstalled(installed, taskHints), query, kindFilter),
        [installed, kindFilter, market, query, taskHints],
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
