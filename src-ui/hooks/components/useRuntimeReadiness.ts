// 某主机上某 backend 能不能直接运行：框架 + Run 依赖，由后端按依赖图解析。
// 替代过去按前端手抄组件链逐个 detect 的做法。

import { useMemo } from 'react';
import { useQueries, useQuery, type UseQueryResult } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import type { ComponentInfo } from '../../core/ipc/generated/domain/ComponentInfo';
import type { RuntimeReadiness } from '../../core/ipc/generated/domain/RuntimeReadiness';
import {
    frameworkComponentFor,
    type ComponentNames,
} from '../../core/domain/components/readiness';
import { useIsHostReachable } from '../remote/useIsHostReachable';

export const RUNTIME_READINESS_QUERY_KEY = 'runtimeReadiness';

export function runtimeReadinessQueryKey(hostId: string, backend: BackendType) {
    return [RUNTIME_READINESS_QUERY_KEY, hostId, frameworkComponentFor(backend)] as const;
}

export interface RuntimeReadinessState {
    readiness: RuntimeReadiness | undefined;
    /** 还没有结果（首次探测中 / 未启用）；有旧结果时后台刷新不算 */
    probing: boolean;
    error: string | null;
}

function toState(q: UseQueryResult<RuntimeReadiness>, enabled: boolean): RuntimeReadinessState {
    return {
        readiness: q.data,
        probing: enabled && q.data === undefined && !q.error,
        error: q.error ? String(q.error) : null,
    };
}

export function useRuntimeReadiness(
    hostId: string | null,
    backend: BackendType,
): RuntimeReadinessState {
    const reachable = useIsHostReachable(hostId);
    const enabled = hostId != null && reachable;
    const q = useQuery({
        queryKey: runtimeReadinessQueryKey(hostId ?? '', backend),
        queryFn: () => componentService.resolveRuntimeReadiness(frameworkComponentFor(backend), hostId!),
        enabled,
        staleTime: 30_000,
    });
    return toState(q, enabled);
}

/** 多个 (host, backend) 一次取，key 为 `${hostId}|${backend}`；数量随入参变 */
export function useRuntimeReadinessMany(
    targets: Array<{ hostId: string; backend: BackendType; enabled: boolean }>,
): Record<string, RuntimeReadinessState> {
    return useQueries({
        queries: targets.map((t) => ({
            queryKey: runtimeReadinessQueryKey(t.hostId, t.backend),
            queryFn: () =>
                componentService.resolveRuntimeReadiness(frameworkComponentFor(t.backend), t.hostId),
            enabled: t.enabled,
            staleTime: 30_000,
        })),
        combine: (results) => {
            const out: Record<string, RuntimeReadinessState> = {};
            targets.forEach((t, i) => {
                out[`${t.hostId}|${t.backend}`] = toState(results[i]!, t.enabled);
            });
            return out;
        },
    });
}

/** 组件 id → 显示名，来自 catalog；未加载时回落 id */
export function useComponentNames(): ComponentNames {
    const q = useQuery({
        queryKey: ['componentCatalog'],
        queryFn: componentService.listComponents,
        staleTime: 5 * 60 * 1000,
    });
    return useMemo(() => {
        const out: ComponentNames = {};
        for (const info of (q.data ?? []) as ComponentInfo[]) {
            out[info.id] = info.display_name;
        }
        return out;
    }, [q.data]);
}
