import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import { dockerHostTargetKey, dockerQueryMatchesHost } from '../../core/domain/docker/engine';
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
    engine?: ContainerEngine,
) {
    const client = useQueryClient();
    const targetKey = dockerHostTargetKey({ hostId, engine });
    const engineArgs = engine ? ([engine] as const) : ([] as const);
    const mutationKey = ['docker', hostId];
    const isBusy = useIsMutating({ mutationKey }) > 0;
    const enabled = hostId.length > 0;
    const interval = autoRefresh && !isBusy ? 15000 : false;
    const statusQuery = useQuery({
        queryKey: ['docker', 'status', targetKey],
        queryFn: () => dockerService.probe(hostId, ...engineArgs),
        enabled,
        retry: false,
        refetchInterval: interval,
    });
    const ready = enabled && !statusQuery.isError && (statusQuery.data?.daemonRunning ?? false);
    const options = { retry: false, refetchInterval: interval } as const;
    const containersQuery = useQuery({
        ...options,
        queryKey: ['docker', 'containers', targetKey],
        queryFn: () => dockerService.listContainers(hostId, ...engineArgs),
        enabled: ready && activeTab === 'containers',
    });
    const imagesQuery = useQuery({
        ...options,
        queryKey: ['docker', 'images', targetKey],
        queryFn: () => dockerService.listImages(hostId, ...engineArgs),
        enabled: ready && activeTab === 'images',
    });
    const networksQuery = useQuery({
        ...options,
        queryKey: ['docker', 'networks', targetKey],
        queryFn: () => dockerService.listNetworks(hostId, ...engineArgs),
        enabled: ready && activeTab === 'networks',
    });
    const volumesQuery = useQuery({
        ...options,
        queryKey: ['docker', 'volumes', targetKey],
        queryFn: () => dockerService.listVolumes(hostId, ...engineArgs),
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
                predicate: (query) => dockerQueryMatchesHost(query.queryKey, target),
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
        mutationFn: async ({
            hostId: target,
            engine,
        }: {
            hostId: string;
            engine?: ContainerEngine;
        }) => {
            const taskId = crypto.randomUUID();
            dockerInstallProgressStore.started(taskId, target);
            dockerActionStore.markInstalling(target, taskId);
            return dockerService.install(target, taskId, {}, engine);
        },
        onSuccess: (_result, target) => invalidateHost(target.hostId),
        onSettled: (_result, _error, target) => dockerActionStore.clearInstalling(target.hostId),
    });
    const actionMutation = useMutation({
        mutationKey,
        retry: false,
        mutationFn: (args: ContainerActionRequest & { hostId: string; engine?: ContainerEngine }) =>
            dockerService.containerAction(
                args.hostId,
                args.name,
                args.action,
                args.force,
                args.engine,
            ),
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
        mutationFn: (args: {
            hostId: string;
            imageRef: string;
            force?: boolean;
            engine?: ContainerEngine;
        }) => dockerService.removeImage(args.hostId, args.imageRef, args.force, args.engine),
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
        mutationFn: (args: ResourceRemoveRequest & { hostId: string; engine?: ContainerEngine }) =>
            dockerService.removeResource(args.hostId, args.kind, args.id, args.engine),
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
        mutationFn: (args: {
            hostId: string;
            name: string;
            removeVolumes: boolean;
            engine?: ContainerEngine;
        }) => dockerService.composeDown(args.hostId, args.name, args.removeVolumes, args.engine),
        onError: (err, args) => failed(args.hostId, '停止部署失败', args.name, err),
        onSettled: (_result, _error, args) => invalidateHost(args.hostId),
    });
    const fetchLogs = useCallback(
        (name: string, tail = 400) =>
            dockerService.logs(
                hostId,
                name,
                tail,
                ...(engine ? ([engine] as const) : ([] as const)),
            ),
        [hostId, engine],
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
        install: () => installMutation.mutateAsync({ hostId, engine }),
        isInstalling: installMutation.isPending,
        containerAction: (args: ContainerActionRequest) =>
            actionMutation.mutate({ ...args, hostId, engine }),
        containerActionAsync: (args: ContainerActionRequest) =>
            actionMutation.mutateAsync({ ...args, hostId, engine }),
        isActing: isBusy,
        removeImage: (args: { imageRef: string; force?: boolean }) =>
            imageMutation.mutate({ ...args, hostId, engine }),
        removeImageAsync: (args: { imageRef: string; force?: boolean }) =>
            imageMutation.mutateAsync({ ...args, hostId, engine }),
        isRemovingImage: imageMutation.isPending,
        removingImageRef:
            imageMutation.isPending && imageMutation.variables?.hostId === hostId
                ? imageMutation.variables.imageRef
                : null,
        removeResourceAsync: (args: ResourceRemoveRequest) =>
            resourceMutation.mutateAsync({ ...args, hostId, engine }),
        composeDown: (args: { name: string; removeVolumes: boolean }) =>
            composeMutation.mutate({ ...args, hostId, engine }),
        isComposingDown: composeMutation.isPending,
        fetchLogs,
    };
}
