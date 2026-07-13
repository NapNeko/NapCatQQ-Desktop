// 远端 release 快照 hook。
//
// 后端默认 1 小时磁盘 TTL；本 hook 用 react-query 做页面级缓存：
// staleTime 5 分钟（避免切页狂拉），gcTime 1 小时。
//
// 用户点「刷新」必须 force=true：同时绕过 react-query 与后端磁盘 TTL，
// 否则只会拿到上一小时内的半残缓存（例如只有 QQ 版本、GitHub 全空）。
//
// retry：全局默认 retry:false；release 网络抖动常见，单独 2 次重试。
// 后端命令永远 Ok（失败字段 None），retry 只覆盖 IPC 层错误。

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useMemo } from 'react';
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
    /// 强制刷新：跳过后端磁盘 TTL + 更新 react-query 缓存。
    refetch: () => void;
}

const RELEASE_QUERY_KEY = ['releaseSnapshot'] as const;
const FIVE_MINUTES = 5 * 60 * 1000;
const ONE_HOUR = 60 * 60 * 1000;

export function useReleases(): UseReleasesResult {
    const queryClient = useQueryClient();
    const query = useQuery({
        queryKey: RELEASE_QUERY_KEY,
        // 默认路径不 force，吃后端 1h 缓存
        queryFn: () => releaseService.getSnapshot(false),
        staleTime: FIVE_MINUTES,
        gcTime: ONE_HOUR,
        retry: 2,
    });

    const snapshot = useMemo(
        () => normalizeReleaseSnapshot(query.data),
        [query.data],
    );

    const refetch = useCallback(() => {
        void queryClient.fetchQuery({
            queryKey: RELEASE_QUERY_KEY,
            queryFn: () => releaseService.getSnapshot(true),
            // 立刻视为新鲜，避免紧接着的 observer 再打一次非 force
            staleTime: FIVE_MINUTES,
        });
    }, [queryClient]);

    return {
        snapshot,
        isLoading: query.isLoading,
        isFetching: query.isFetching,
        error: query.error as Error | null,
        refetch,
    };
}
