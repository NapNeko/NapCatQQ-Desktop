// 详情和日志按主机、容器 ID 隔离；关闭对话框后停止定时读取。
import { useQuery } from '@tanstack/react-query';
import { dockerService } from '../../core/services/docker.service';

export function useDockerContainer(hostId: string, id: string) {
    const details = useQuery({
        queryKey: ['docker', 'details', hostId, id],
        queryFn: () => dockerService.containerDetails(hostId, id),
        retry: false,
        staleTime: 0,
        refetchInterval: 15000,
    });
    const stats = useQuery({
        queryKey: ['docker', 'stats', hostId, id],
        queryFn: () => dockerService.containerStats(hostId, id),
        enabled: !details.isError && details.data?.state === 'running',
        retry: false,
        staleTime: 0,
        refetchInterval: 5000,
    });
    return { details, stats };
}

export function useDockerLogs(hostId: string, id: string, tail: number, autoRefresh: boolean) {
    return useQuery({
        queryKey: ['docker', 'logs', hostId, id, tail],
        queryFn: () => dockerService.logs(hostId, id, tail),
        retry: false,
        staleTime: 0,
        refetchInterval: autoRefresh ? 5000 : false,
    });
}
