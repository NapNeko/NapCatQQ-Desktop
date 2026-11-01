import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import { dockerHostTargetKey } from '../../core/domain/docker/engine';
// 详情和日志按主机、容器 ID 隔离；关闭对话框后停止定时读取。
import { useQuery } from '@tanstack/react-query';
import { dockerService } from '../../core/services/docker.service';

export function useDockerContainer(hostId: string, id: string, engine?: ContainerEngine) {
    const target = dockerHostTargetKey({ hostId, engine });
    const engineArgs = engine ? ([engine] as const) : ([] as const);
    const details = useQuery({
        queryKey: ['docker', 'details', target, id],
        queryFn: () => dockerService.containerDetails(hostId, id, ...engineArgs),
        retry: false,
        staleTime: 0,
        refetchInterval: 15000,
    });
    const stats = useQuery({
        queryKey: ['docker', 'stats', target, id],
        queryFn: () => dockerService.containerStats(hostId, id, ...engineArgs),
        enabled: !details.isError && details.data?.state === 'running',
        retry: false,
        staleTime: 0,
        refetchInterval: 5000,
    });
    return { details, stats };
}

export function useDockerLogs(
    hostId: string,
    id: string,
    tail: number,
    autoRefresh: boolean,
    engine?: ContainerEngine,
) {
    const target = dockerHostTargetKey({ hostId, engine });
    const engineArgs = engine ? ([engine] as const) : ([] as const);
    return useQuery({
        queryKey: ['docker', 'logs', target, id, tail],
        queryFn: () => dockerService.logs(hostId, id, tail, ...engineArgs),
        retry: false,
        staleTime: 0,
        refetchInterval: autoRefresh ? 5000 : false,
    });
}
