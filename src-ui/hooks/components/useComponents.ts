// Components 页面主 hook。
//
// 职责：
//   1. 拉一次 list_components（react-query 缓存）
//   2. 对每个 (component, host) 组合 detect_component，merge 成 ComponentRow[]
//   3. 暴露 refetch / lastError 给 UI
//
// 主机列表当前来自 mock；以后接入 Remote 页时换成 useRemoteHosts() 并集
// `local` host。frontend-layering：本 hook 唯一允许调 service 的位置。

import { useQuery, useQueries } from '@tanstack/react-query';
import { useMemo } from 'react';
import { componentService } from '../../core/services/component.service';
import {
    deriveStatus,
    splitByCategory,
    type ComponentRow,
    type ComponentsView,
    type HostInfo,
} from '../../core/domain/components/types';
import { mockHosts } from '../../core/ipc/mock/component.mock';

// TODO: 接入 Remote 页 useRemoteHosts() 后替换。当前直接用 mock 主机列表，
// 真后端返回的 detect supported=false 时 UI 会显示"不支持"，行为正确。
function useKnownHosts(): HostInfo[] {
    return mockHosts;
}

export interface UseComponentsResult {
    view: ComponentsView;
    isLoading: boolean;
    error: Error | null;
    /// 触发整页重新拉一次。
    refetch: () => void;
}

export function useComponents(): UseComponentsResult {
    const hosts = useKnownHosts();

    const catalogQuery = useQuery({
        queryKey: ['componentCatalog'],
        queryFn: componentService.listComponents,
        staleTime: 5 * 60 * 1000,
    });

    const components = catalogQuery.data ?? [];

    // 对每个 (component, host) 发一个 detect。
    // useQueries 让 react-query 自己管理缓存 + 并发；keys 稳定。
    const detectQueries = useQueries({
        queries: components.flatMap((c) =>
            hosts.map((h) => ({
                queryKey: ['componentDetect', c.id, h.host_id],
                queryFn: () => componentService.detectComponent(c.id, h.host_id),
                staleTime: 30 * 1000,
            })),
        ),
    });

    // 把 catalog × hosts × detect 三者合成 ComponentRow[]
    const view = useMemo<ComponentsView>(() => {
        if (components.length === 0) {
            return { framework: [], runtimeDep: [], selfApp: [] };
        }

        // 用 host 数量 + 平铺索引把 detectQueries 切回 (component → host[])
        const hostCount = hosts.length;
        const rows: ComponentRow[] = components.map((info, ci) => ({
            info,
            rows: hosts.map((host, hi) => {
                const detectQuery = detectQueries[ci * hostCount + hi];
                return {
                    component_id: info.id,
                    host,
                    status: deriveStatus(host, info, detectQuery?.data ?? null),
                };
            }),
        }));

        return splitByCategory(rows);
    }, [components, hosts, detectQueries]);

    const error =
        (catalogQuery.error as Error | null) ??
        (detectQueries.find((q) => q.error)?.error as Error | undefined) ??
        null;

    const isLoading =
        catalogQuery.isLoading ||
        detectQueries.some((q) => q.isLoading);

    return {
        view,
        isLoading,
        error,
        refetch: () => {
            void catalogQuery.refetch();
            for (const q of detectQueries) void q.refetch();
        },
    };
}
