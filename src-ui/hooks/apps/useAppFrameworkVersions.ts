// 应用端框架的「可安装版本」查询。
//
// 后端 AppFrameworkAdapter::available_versions 对不支持的框架返回 null，
// 这里原样透出：调用方拿到 null 就不要显示版本选择器。

import { useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import type { PackageVersions } from '../../core/ipc/types';

export const appFrameworkVersionsKey = (frameworkId: string) =>
    ['appFrameworkVersions', frameworkId] as const;

/**
 * 一批实例涉及的框架 → 上游最新正式版。
 *
 * 列表页每张卡片都要这个提示，但同一框架不该查多次。这里按**去重后的框架集合**
 * 各查一次；某个框架查不到（不支持 / 网络失败）就不进 map，调用方按「不提示」处理。
 */
export function useAppFrameworkLatestVersions(
    instances: readonly { framework_id: string }[],
): Map<string, string> {
    // 依赖是「框架集合」，不是 instances 数组本身：否则每次实例状态变化
    // （启停、安装进度）都会重建 key，把所有框架的查询全部重跑
    const frameworkIds = useMemo(
        () => [...new Set(instances.map((i) => i.framework_id))].sort(),
        [instances],
    );
    const queries = useQueries({
        queries: frameworkIds.map((id) => ({
            queryKey: appFrameworkVersionsKey(id),
            queryFn: () => appFrameworkService.listVersions(id),
            staleTime: 10 * 60 * 1000,
            retry: false,
        })),
    });
    return useMemo(() => {
        const out = new Map<string, string>();
        frameworkIds.forEach((id, idx) => {
            const latest = (queries[idx]?.data as PackageVersions | null | undefined)?.latest;
            if (latest) out.set(id, latest);
        });
        return out;
    }, [frameworkIds, queries]);
}

/**
 * 某框架的可安装版本；null = 该框架不支持按版本安装。
 *
 * 缓存时间给得长一些：PyPI 版本清单变化不频繁，而打开对话框 / 详情页会反复要。
 */
export function useAppFrameworkVersions(frameworkId: string | null | undefined) {
    return useQuery<PackageVersions | null, Error>({
        queryKey: appFrameworkVersionsKey(frameworkId ?? ''),
        queryFn: () => appFrameworkService.listVersions(frameworkId as string),
        enabled: !!frameworkId,
        staleTime: 10 * 60 * 1000,
        retry: false,
    });
}
