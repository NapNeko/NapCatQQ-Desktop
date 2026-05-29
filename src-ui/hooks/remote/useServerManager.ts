// ServerManager CRUD + 连接测试的 React 适配层。
// 远端组件部署走组件页 (run_component_action with host_id = "remote:<id>")。

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ProbeReport } from '../../core/ipc/generated/domain/ProbeReport';

export function useServerManager() {
    const queryClient = useQueryClient();

    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
    });

    const addMutation = useMutation({
        mutationFn: (args: { profile: ServerProfile; password?: string }) =>
            serverService.add(args.profile, args.password),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
        },
    });

    const updateMutation = useMutation({
        mutationFn: (args: { profile: ServerProfile; password?: string }) =>
            serverService.update(args.profile, args.password),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
        },
    });

    const deleteMutation = useMutation({
        mutationFn: (id: string) => serverService.delete(id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
        },
    });

    const testMutation = useMutation({
        mutationFn: (args: { id: string; password?: string }) =>
            serverService.testConnection(args.id, args.password),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['servers'] });
        },
    });

    return {
        servers: serversQuery.data ?? [],
        isLoading: serversQuery.isLoading,
        refetch: serversQuery.refetch,

        addServer: addMutation.mutate,
        isAdding: addMutation.isPending,

        updateServer: updateMutation.mutate,
        isUpdating: updateMutation.isPending,

        deleteServer: deleteMutation.mutate,
        isDeleting: deleteMutation.isPending,

        testConnection: testMutation.mutate,
        isTesting: testMutation.isPending,
        testResult: testMutation.data as ProbeReport | undefined,
    };
}
