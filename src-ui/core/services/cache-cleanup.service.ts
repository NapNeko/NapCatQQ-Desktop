import { invoke, isTauri } from '../ipc/transport';
import { cacheCleanupMock } from '../ipc/mock/cache-cleanup.mock';
import type { CacheCategory, CacheCleanupResult, CacheScan } from '../ipc/types';

export const cacheCleanupService = {
    scan: async (): Promise<CacheScan> =>
        isTauri ? invoke<CacheScan>('scan_download_cache') : cacheCleanupMock.scan(),
    clean: async (categories: CacheCategory[]): Promise<CacheCleanupResult> =>
        isTauri
            ? invoke<CacheCleanupResult>('clean_download_cache', { categories })
            : cacheCleanupMock.clean(categories),
};
