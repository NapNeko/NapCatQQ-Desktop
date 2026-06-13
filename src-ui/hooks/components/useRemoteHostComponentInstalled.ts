// 单台主机上运行时组件是否已安装（供 Bot 配置「远程 + 直接运行」缺项提示）。

import { useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';
import type { ComponentId } from '../../core/ipc/generated/domain/ComponentId';
import {
    remoteDirectRunChain,
    type RemoteDirectRunComponentId,
} from '../../core/domain/bot/remote-direct-run-deps';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';

const ALL_PROBE_IDS: RemoteDirectRunComponentId[] = [
    'qq',
    'napcat',
    'nodejs',
    'snowluma',
    'novnc',
];

function installedFromDetect(detected: unknown): boolean {
    return detected != null && typeof detected === 'object';
}

export function useRemoteHostComponentInstalled(
    hostId: string | null,
    backendType: BackendType,
): Partial<Record<RemoteDirectRunComponentId, boolean | undefined>> {
    useQuery({
        queryKey: ['componentCatalog'],
        queryFn: componentService.listComponents,
        staleTime: 5 * 60 * 1000,
    });

    const chain = useMemo(() => remoteDirectRunChain(backendType), [backendType]);

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
            Record<RemoteDirectRunComponentId, boolean | undefined>
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