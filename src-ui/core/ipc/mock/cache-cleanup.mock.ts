import type { CacheCategory, CacheCleanupResult, CacheScan } from '../types';

const initial: CacheScan = {
    busy: false,
    categories: [
        {
            category: 'installers',
            files: 2,
            bytes: BigInt(240 * 1024 * 1024),
            skipped: 0,
            warnings: [],
        },
        {
            category: 'desktop_updates',
            files: 1,
            bytes: BigInt(48 * 1024 * 1024),
            skipped: 1,
            warnings: [],
        },
        {
            category: 'download_residues',
            files: 5,
            bytes: BigInt(72 * 1024 * 1024),
            skipped: 2,
            warnings: [],
        },
    ],
};
let snapshot = structuredClone(initial);

export const cacheCleanupMock = {
    scan: async (): Promise<CacheScan> => structuredClone(snapshot),
    clean: async (categories: CacheCategory[]): Promise<CacheCleanupResult> => {
        const selected = snapshot.categories.filter((group) => categories.includes(group.category));
        const result: CacheCleanupResult = {
            deleted_files: selected.reduce((sum, group) => sum + group.files, 0),
            deleted_bytes: selected.reduce((sum, group) => sum + group.bytes, BigInt(0)),
            skipped_files: selected.reduce((sum, group) => sum + group.skipped, 0),
            failed_files: 0,
            warnings: [],
        };
        snapshot = {
            ...snapshot,
            categories: snapshot.categories.map((group) =>
                categories.includes(group.category)
                    ? { ...group, files: 0, bytes: BigInt(0) }
                    : group,
            ),
        };
        return result;
    },
};
