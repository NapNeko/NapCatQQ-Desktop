import { useQuery } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import type { CacheCategory, CacheCleanupResult } from '../../core/ipc/types';
import { cacheCleanupService } from '../../core/services/cache-cleanup.service';

export function useCacheCleanup(open: boolean) {
    const scan = useQuery({
        queryKey: ['downloadCache'],
        queryFn: cacheCleanupService.scan,
        enabled: open,
        retry: false,
        refetchOnWindowFocus: false,
        staleTime: 0,
    });
    const running = useRef(false);
    const [busy, setBusy] = useState(false);
    const [result, setResult] = useState<CacheCleanupResult | null>(null);
    const [error, setError] = useState<string | null>(null);

    const clean = async (categories: CacheCategory[]) => {
        if (running.current || categories.length === 0) return;
        running.current = true;
        setBusy(true);
        setError(null);
        setResult(null);
        try {
            setResult(await cacheCleanupService.clean(categories));
            await scan.refetch();
        } catch (error) {
            setError(error instanceof Error ? error.message : String(error));
            await scan.refetch();
        } finally {
            running.current = false;
            setBusy(false);
        }
    };
    return { scan, clean, busy, result, error };
}
