import { useSyncExternalStore } from 'react';
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { dockerQueryMatchesHost } from '../../core/domain/docker/engine';
import { dockerService } from '../../core/services/docker.service';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import { dockerActionStore, dockerPullTargetKey } from './dockerActionStore';
import { useDockerQueryNotice } from './useDockerQueryNotice';
import { pushErrorBar } from '../ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import { dismissInfoBar } from '../ui/globalInfoBarStore';

function hasDeployment(hostId: string) {
    const state = dockerActionStore.getSnapshot();
    return (
        !!state.installingByHost[hostId] ||
        !!state.pullingByTarget[dockerPullTargetKey(hostId, 'napcat')] ||
        !!state.pullingByTarget[dockerPullTargetKey(hostId, 'snowluma')]
    );
}

export function useContainerEngine(hostId: string, enabled = true) {
    const client = useQueryClient();
    const deploying = useSyncExternalStore(dockerActionStore.subscribe, () =>
        hasDeployment(hostId),
    );
    const acting = useIsMutating({ mutationKey: ['docker', hostId] }) > 0;
    const query = useQuery({
        queryKey: ['docker', 'engine', hostId],
        queryFn: () => dockerService.getEngine(hostId),
        enabled: !!hostId && enabled,
        staleTime: Infinity,
        retry: false,
    });
    useDockerQueryNotice(
        `container-engine:${hostId}`,
        '读取容器引擎失败',
        enabled ? query.error : null,
    );
    const mutation = useMutation({
        retry: false,
        mutationFn: ({ hostId: target, engine }: { hostId: string; engine: ContainerEngine }) => {
            if (hasDeployment(target) || client.isMutating({ mutationKey: ['docker', target] })) {
                throw new Error('该主机有容器操作尚未结束，请稍后切换');
            }
            return dockerService.setEngine(target, engine);
        },
        onSuccess: async (_, { hostId: target, engine }) => {
            dismissInfoBar(`key:container-engine-switch:${target}`);
            const resources = {
                predicate: (q: { queryKey: readonly unknown[] }) =>
                    dockerQueryMatchesHost(q.queryKey, target) && q.queryKey[1] !== 'engine',
            };
            await client.cancelQueries(resources);
            client.setQueryData(['docker', 'engine', target], engine);
            await client.resetQueries(resources);
            await client.invalidateQueries({
                predicate: (q) => q.queryKey[0] === 'botConfig' || q.queryKey[0] === 'botConfigs',
            });
        },
        onError: (error, { hostId: target }) =>
            pushErrorBar({
                key: `container-engine-switch:${target}`,
                title: '无法切换容器引擎',
                raw: errorText(error),
            }),
    });
    return {
        engine: query.data,
        loading: enabled && (query.isPending || (!query.data && query.isFetching)),
        switching: mutation.isPending,
        pendingEngine: mutation.isPending ? mutation.variables.engine : undefined,
        busy: deploying || acting,
        error: query.error,
        refetch: query.refetch,
        select: (engine: ContainerEngine) => mutation.mutateAsync({ hostId, engine }),
    };
}
