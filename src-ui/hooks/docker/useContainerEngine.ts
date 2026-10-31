import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { dockerQueryMatchesHost } from '../../core/domain/docker/engine';
import { dockerService } from '../../core/services/docker.service';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';

export function useContainerEngine(hostId: string, enabled = true) {
    const client = useQueryClient();
    const query = useQuery({
        queryKey: ['docker', 'engine', hostId],
        queryFn: () => dockerService.getEngine(hostId),
        enabled: !!hostId && enabled,
        staleTime: Infinity,
    });
    const mutation = useMutation({
        mutationFn: (engine: ContainerEngine) => dockerService.setEngine(hostId, engine),
        onSuccess: async (_, engine) => {
            client.setQueryData(['docker', 'engine', hostId], engine);
            // 两个引擎的镜像和容器完全独立，切换后立即移除旧缓存再重新探测。
            await client.resetQueries({
                predicate: (q) =>
                    dockerQueryMatchesHost(q.queryKey, hostId) && q.queryKey[1] !== 'engine',
            });
            await client.invalidateQueries({
                predicate: (q) => q.queryKey[0] === 'botConfig' || q.queryKey[0] === 'botConfigs',
            });
        },
    });
    return {
        engine: query.data ?? 'docker',
        loading: enabled && query.isPending,
        switching: mutation.isPending,
        pendingEngine: mutation.isPending ? mutation.variables : undefined,
        error: query.error,
        refetch: query.refetch,
        select: mutation.mutateAsync,
    };
}
