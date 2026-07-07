// 轻量 Docker 状态探测：只用于全局入口显隐，不拉容器/镜像列表。

import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import { dockerService } from '../../core/services/docker.service';
import type { DockerStatus } from '../../core/ipc/types';

export function useDockerStatusByHost(hostIds: string[]): Record<string, DockerStatus | undefined> {
    const queries = useQueries({
        queries: hostIds.map((hostId) => ({
            queryKey: ['docker', 'status', hostId],
            queryFn: () => dockerService.probe(hostId),
            staleTime: 30 * 1000,
        })),
    });

    return useMemo(() => {
        const out: Record<string, DockerStatus | undefined> = {};
        hostIds.forEach((hostId, index) => {
            out[hostId] = queries[index]?.data;
        });
        return out;
    }, [hostIds, queries]);
}
