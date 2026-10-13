// Docker 查询和操作绑定发起时的主机，切换主机后仍只刷新原主机。
import { useCallback } from 'react';
import { useQuery, useMutation, useQueryClient, useIsMutating } from '@tanstack/react-query';
import { dockerService } from '../../core/services/docker.service';
import { dockerActionStore } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import { imageRemoveFailureHint } from '../../core/domain/docker/status';
import type { ContainerAction, DockerResourceKind } from '../../core/ipc/types';

export type DockerTab = 'containers' | 'images' | 'networks' | 'volumes';
export type ContainerActionRequest = {
    name: string;
    displayName?: string;
    action: ContainerAction;
    force?: boolean;
};
export type ResourceRemoveRequest = { kind: DockerResourceKind; id: string; name: string };

const ACTION_VERB: Record<ContainerAction, string> = {
    start: '启动',
    stop: '停止',
    restart: '重启',
    pause: '暂停',
    unpause: '恢复',
    remove: '删除',
};

export function useDocker(
    hostId: string,
    activeTab: DockerTab = 'containers',
    autoRefresh = false,
) {
    const client = useQueryClient();
    const mutationKey = ['docker', hostId];
    const isBusy = useIsMutating({ mutationKey }) > 0;
    const enabled = hostId.length > 0;
    const interval = autoRefresh && !isBusy ? 15000 : false;
    const statusQuery = useQuery({
        queryKey: ['docker', 'status', hostId],
        queryFn: () => dockerService.probe(hostId),
        enabled,
        retry: false,
        refetchInterval: interval,
    });
    const ready = enabled && !statusQuery.isError && (statusQuery.data?.daemonRunning ?? false);
    const options = { retry: false, refetchInterval: interval } as const;
    const containersQuery = useQuery({
        ...options,
        queryKey: ['docker', 'containers', hostId],
        queryFn: () => dockerService.listContainers(hostId),
        enabled: ready && activeTab === 'containers',
    });
    const imagesQuery = useQuery({
        ...options,
        queryKey: ['docker', 'images', hostId],
        queryFn: () => dockerService.listImages(hostId),
        enabled: ready && activeTab === 'images',
    });
    const networksQuery = useQuery({
        ...options,
        queryKey: ['docker', 'networks', hostId],
        queryFn: () => dockerService.listNetworks(hostId),
        enabled: ready && activeTab === 'networks',
    });
    const volumesQuery = useQuery({
        ...options,
        queryKey: ['docker', 'volumes', hostId],
        queryFn: () => dockerService.listVolumes(hostId),
        enabled: ready && activeTab === 'volumes',
    });
    const activeQuery = {
        containers: containersQuery,
        images: imagesQuery,
        networks: networksQuery,
        volumes: volumesQuery,
    }[activeTab];

    const invalidateHost = useCallback(
        (target: string) =>
            client.invalidateQueries({
                predicate: (query) =>
                    query.queryKey[0] === 'docker' && query.queryKey[2] === target,
            }),
        [client],
    );
    const success = (target: string, title: string, name: string) => {
        pushInfoBar({
            key: `docker:${target}:${name}`,
            tone: 'success',
            title,
            content: name,
            autoDismissMs: 4000,
        });
    };
    const failed = (target: string, title: string, name: string, err: unknown, hint?: string) => {
        pushErrorBar({
            key: `docker:${target}:${name}`,
            title,
            raw: `${name}：${errorText(err)}`,
            content: hint || undefined,
        });
    };

    const installMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: async (target: string) => {
            const taskId = crypto.randomUUID();
            dockerInstallProgressStore.started(taskId, target);
            dockerActionStore.markInstalling(target, taskId);
            return dockerService.install(target, taskId);
        },
        onSuccess: (_result, target) => invalidateHost(target),
        onSettled: (_result, _error, target) => dockerActionStore.clearInstalling(target),
    });
    const actionMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: (args: ContainerActionRequest & { hostId: string }) =>
            dockerService.containerAction(args.hostId, args.name, args.action, args.force),
        onSuccess: (_result, args) =>
            success(
                args.hostId,
                `容器已${ACTION_VERB[args.action]}`,
                args.displayName ?? args.name,
            ),
        onError: (err, args) =>
            failed(
                args.hostId,
                `容器${ACTION_VERB[args.action]}失败`,
                args.displayName ?? args.name,
                err,
            ),
        // 超时不代表远端没执行，失败后也回读状态，绝不自动重试写操作。
        onSettled: (_result, _error, args) => invalidateHost(args.hostId),
    });
    const imageMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: (args: { hostId: string; imageRef: string; force?: boolean }) =>
            dockerService.removeImage(args.hostId, args.imageRef, args.force),
        onSuccess: (_result, args) => success(args.hostId, '镜像已删除', args.imageRef),
        onError: (err, args) =>
            failed(
                args.hostId,
                '删除镜像失败',
                args.imageRef,
                err,
                imageRemoveFailureHint(errorText(err)),
            ),
        onSettled: (_result, _error, args) => invalidateHost(args.hostId),
    });
    const resourceMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: (args: ResourceRemoveRequest & { hostId: string }) =>
            dockerService.removeResource(args.hostId, args.kind, args.id),
        onSuccess: (_result, args) =>
            success(
                args.hostId,
                args.kind === 'network' ? '网络已删除' : '数据卷已删除',
                args.name,
            ),
        onError: (err, args) => failed(args.hostId, '删除资源失败', args.name, err),
        onSettled: (_result, _error, args) => invalidateHost(args.hostId),
    });
    const composeMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: (args: { hostId: string; name: string; removeVolumes: boolean }) =>
            dockerService.composeDown(args.hostId, args.name, args.removeVolumes),
        onError: (err, args) => failed(args.hostId, '停止部署失败', args.name, err),
        onSettled: (_result, _error, args) => invalidateHost(args.hostId),
    });
    const fetchLogs = useCallback(
        (name: string, tail = 400) => dockerService.logs(hostId, name, tail),
        [hostId],
    );

    return {
        status: statusQuery.data,
        statusError: statusQuery.error,
        ready,
        isProbing: statusQuery.isFetching,
        isRefreshing: statusQuery.isFetching || activeQuery.isFetching,
        listError: activeQuery.error,
        isLoading: activeQuery.isLoading,
        containers: containersQuery.data ?? [],
        isLoadingList: containersQuery.isLoading,
        images: imagesQuery.data ?? [],
        isLoadingImages: imagesQuery.isLoading,
        networks: networksQuery.data ?? [],
        volumes: volumesQuery.data ?? [],
        refetch: () => {
            void invalidateHost(hostId);
        },
        install: () => installMutation.mutateAsync(hostId),
        isInstalling: installMutation.isPending,
        containerAction: (args: ContainerActionRequest) =>
            actionMutation.mutate({ ...args, hostId }),
        containerActionAsync: (args: ContainerActionRequest) =>
            actionMutation.mutateAsync({ ...args, hostId }),
        isActing: isBusy,
        removeImage: (args: { imageRef: string; force?: boolean }) =>
            imageMutation.mutate({ ...args, hostId }),
        removeImageAsync: (args: { imageRef: string; force?: boolean }) =>
            imageMutation.mutateAsync({ ...args, hostId }),
        isRemovingImage: imageMutation.isPending,
        removingImageRef:
            imageMutation.isPending && imageMutation.variables?.hostId === hostId
                ? imageMutation.variables.imageRef
                : null,
        removeResourceAsync: (args: ResourceRemoveRequest) =>
            resourceMutation.mutateAsync({ ...args, hostId }),
        composeDown: (args: { name: string; removeVolumes: boolean }) =>
            composeMutation.mutate({ ...args, hostId }),
        isComposingDown: composeMutation.isPending,
        fetchLogs,
    };
}
