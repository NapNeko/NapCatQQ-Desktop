// 远端 release 快照 hook。
//
// 后端实装 1 小时 TTL 缓存，这里就用 react-query 做一层"页面级缓存 + 自动
// stale 后台刷新"：staleTime 5 分钟（避免页面切换频繁拉），cacheTime 1 小时
// （和后端缓存对齐）。
//
// 返回 ReleaseSnapshotView：bigint 已转 number，UI 层直接可用。

import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import { releaseService } from '../../core/services/release.service';
import {
    normalizeReleaseSnapshot,
    type ReleaseSnapshotView,
} from '../../core/domain/release/normalize';

export interface UseReleasesResult {
    /// 永远返回非 null 视图（拉取失败 / 还没拉到时所有字段为 null）。
    snapshot: ReleaseSnapshotView;
    isLoading: boolean;
    isFetching: boolean;
    error: Error | null;
    /// 手动触发刷新（绕过 staleTime）。
    refetch: () => void;
}

const FIVE_MINUTES = 5 * 60 * 1000;
const ONE_HOUR = 60 * 60 * 1000;

export function useReleases(): UseReleasesResult {
    const query = useQuery({
        queryKey: ['releaseSnapshot'],
        queryFn: releaseService.getSnapshot,
        staleTime: FIVE_MINUTES,
        gcTime: ONE_HOUR,
    });

    const snapshot = useMemo(
        () => normalizeReleaseSnapshot(query.data),
        [query.data],
    );

    return {
        snapshot,
        isLoading: query.isLoading,
        isFetching: query.isFetching,
        error: query.error as Error | null,
        refetch: () => {
            void query.refetch();
        },
    };
}
