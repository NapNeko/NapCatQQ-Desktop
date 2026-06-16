// 单台主机上运行时组件是否已安装（支持本机 'local' 和远端 'remote:xxx'）。
// 供 Bot 配置页「远程/本地 + 直接运行」以及组件相关门禁使用。
//
// 复用 react-query 缓存，与 ComponentsPage / useComponents 共享数据，
// 避免重复探测。

import { useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';
import type { ComponentId } from '../../core/ipc/generated/domain/ComponentId';
import {
    remoteDirectRunChain,
    localDirectRunChain,
    type DirectRunComponentId,
} from '../../core/domain/bot/remote-direct-run-deps';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';

const ALL_PROBE_IDS: DirectRunComponentId[] = [
    'qq',
    'napcat',
    'nodejs',
    'snowluma',
    'novnc',
];

function installedFromDetect(detected: unknown): boolean {
    return detected != null && typeof detected === 'object';
}

/**
 * 判断给定 hostId 是否为本机。
 */
function isLocalHost(hostId: string | null): boolean {
    return hostId === 'local';
}

/**
 * 通用 hook：给定 hostId（'local' 或 'remote:xxx'）和 backendType，
 * 返回该主机上对应 backend 启动**直接运行**模式所需的组件安装状态。
 *
 * 重要区分：
 * - 本地直接运行（hostId = 'local'）：SnowLuma 包自带 Node，不探测 'nodejs'。
 * - 远程直接运行：SnowLuma 需要独立的 Node + noVNC。
 */
export function useHostComponentInstalled(
    hostId: string | null,
    backendType: BackendType,
): Partial<Record<DirectRunComponentId, boolean | undefined>> {
    useQuery({
        queryKey: ['componentCatalog'],
        queryFn: componentService.listComponents,
        staleTime: 5 * 60 * 1000,
    });

    const isLocal = isLocalHost(hostId);
    const chain = isLocal
        ? localDirectRunChain(backendType)
        : remoteDirectRunChain(backendType);

    const queries = useQueries({
        queries: ALL_PROBE_IDS.map((componentId) => ({
            queryKey: ['componentDetect', componentId, hostId],
            queryFn: () =>
                componentService.detectComponent(
                    componentId as ComponentId,
                    hostId!,
                ),
            enabled: hostId != null && chain.includes(componentId),
            staleTime: 30 * 1000,
        })),
    });

    return useMemo(() => {
        const out: Partial<
            Record<DirectRunComponentId, boolean | undefined>
        > = {};
        if (!hostId) return out;

        ALL_PROBE_IDS.forEach((id, i) => {
            if (!chain.includes(id)) return;
            const q = queries[i];
            if (!q || (q.isFetching && q.data === undefined)) {
                out[id] = undefined;
                return;
            }
            if (q.error) {
                out[id] = undefined;
                return;
            }
            if (q.data === undefined) {
                out[id] = undefined;
                return;
            }
            out[id] = installedFromDetect(q.data.detected);
        });
        return out;
    }, [hostId, chain, queries]);
}

/** 旧名字兼容导出（历史代码仍可使用）。 */
export const useRemoteHostComponentInstalled = useHostComponentInstalled;