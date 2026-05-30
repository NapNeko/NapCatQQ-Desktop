// 多主机 Docker 探测 hook，给组件页的 Docker 卡片 + NapCat/SnowLuma 部署形态用。
//
// 跟单主机的 useDocker 区别：这里一次性对所有已知主机 probe docker 状态（用
// useQueries），返回 host_id → DockerStatus 映射，外加 install / deploy mutation。
//
// frontend-layering：唯一允许调 dockerService 的位置之一（与 useDocker 并列）。

import { useCallback, useMemo } from 'react';
import { useMutation, useQueries, useQueryClient } from '@tanstack/react-query';

import { dockerService } from '../../core/services/docker.service';
import { openExternalUrl } from '../../core/ipc/transport';
import type {
    DeployedContainer,
    DockerDeploySpec,
    DockerStatus,
} from '../../core/ipc/types';

/// Docker Desktop 下载页，Windows / macOS 走手动安装时引导用。
const DOCKER_DESKTOP_URL = 'https://www.docker.com/products/docker-desktop/';

export interface UseDockerHostsResult {
    /// host_id → 该主机的 docker 探测结果（还没回来时 undefined）。
    statusByHost: Record<string, DockerStatus | undefined>;
    /// host_id → 是否正在探测。
    probingByHost: Record<string, boolean>;
    /// 刷新所有主机的 docker 探测。
    refetch: () => void;
    /// 在某主机上装 / 起 docker，返回结果文案。
    install: (hostId: string) => Promise<string>;
    isInstalling: boolean;
    /// 打开 Docker Desktop 下载页（Windows / macOS 手动安装引导用）。
    openDownloadPage: () => Promise<void>;
    /// 在某主机上部署一个容器。
    deploy: (hostId: string, spec: DockerDeploySpec) => Promise<DeployedContainer>;
    isDeploying: boolean;
}

export function useDockerHosts(hostIds: string[]): UseDockerHostsResult {
    const queryClient = useQueryClient();

    const queries = useQueries({
        queries: hostIds.map((hostId) => ({
            queryKey: ['docker', 'status', hostId],
            queryFn: () => dockerService.probe(hostId),
            staleTime: 30 * 1000,
        })),
    });

    const statusByHost = useMemo<Record<string, DockerStatus | undefined>>(() => {
        const out: Record<string, DockerStatus | undefined> = {};
        hostIds.forEach((id, i) => {
            out[id] = queries[i]?.data;
        });
        return out;
    }, [hostIds, queries]);

    const probingByHost = useMemo<Record<string, boolean>>(() => {
        const out: Record<string, boolean> = {};
        hostIds.forEach((id, i) => {
            out[id] = queries[i]?.isLoading ?? false;
        });
        return out;
    }, [hostIds, queries]);

    const invalidate = useCallback(() => {
        queryClient.invalidateQueries({ queryKey: ['docker'] });
    }, [queryClient]);

    const installMutation = useMutation({
        mutationFn: (hostId: string) => dockerService.install(hostId),
        onSuccess: invalidate,
    });

    const deployMutation = useMutation({
        mutationFn: (args: { hostId: string; spec: DockerDeploySpec }) =>
            dockerService.deploy(args.hostId, args.spec),
        onSuccess: invalidate,
    });

    return {
        statusByHost,
        probingByHost,
        refetch: invalidate,
        install: installMutation.mutateAsync,
        isInstalling: installMutation.isPending,
        openDownloadPage: () => openExternalUrl(DOCKER_DESKTOP_URL),
        deploy: (hostId, spec) => deployMutation.mutateAsync({ hostId, spec }),
        isDeploying: deployMutation.isPending,
    };
}
