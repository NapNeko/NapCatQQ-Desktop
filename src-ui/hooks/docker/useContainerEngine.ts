import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { dockerService } from '../../core/services/docker.service';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';

export function useContainerEngine(hostId: string) {
    const client = useQueryClient();
    const query = useQuery({
        queryKey: ['docker', 'engine', hostId],
        queryFn: () => dockerService.getEngine(hostId),
        enabled: !!hostId,
        staleTime: Infinity,
    });
    const mutation = useMutation({
        mutationFn: (engine: ContainerEngine) => dockerService.setEngine(hostId, engine),
        onSuccess: async (_, engine) => {
            client.setQueryData(['docker', 'engine', hostId], engine);
            // 两个引擎的镜像和容器完全独立，切换后立即移除旧缓存再重新探测。
            await client.resetQueries({
                predicate: (q) =>
                    q.queryKey[0] === 'docker' &&
                    q.queryKey[2] === hostId &&
                    q.queryKey[1] !== 'engine',
            });
        },
    });
    return {
        engine: query.data ?? 'docker',
        loading: query.isPending,
        switching: mutation.isPending,
        select: mutation.mutateAsync,
    };
}
