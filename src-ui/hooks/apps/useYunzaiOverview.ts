// 云崽概览页的数据查询：判断「还差插件吗」要读已装插件列表。
// 键与 stale/gc 直接复用商店那套（appStoreInstalledKey），和插件页共用一份缓存，切过去不重读。

import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { APP_STORE_GC_MS, APP_STORE_STALE_MS, appStoreInstalledKey } from './appStoreQuery';

export function useYunzaiInstalledPlugins(instanceId: string) {
    return useQuery({
        queryKey: appStoreInstalledKey(instanceId, 'plugin'),
        queryFn: () => appFrameworkService.listStoreInstalled(instanceId, 'plugin'),
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });
}
