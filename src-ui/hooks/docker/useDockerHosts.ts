// 多主机 Docker 探测 hook，给组件页的 Docker 卡片 + NapCat/SnowLuma 部署形态用。
//
// 跟单主机的 useDocker 区别：这里一次性对所有已知主机 probe docker 状态（用
// useQueries），返回 host_id → DockerStatus 映射，外加 install / deploy mutation。
//
// frontend-layering：唯一允许调 dockerService 的位置之一（与 useDocker 并列）。

import { useCallback, useMemo } from 'react';
import { useSyncExternalStore } from 'react';
import { useMutation, useQueries, useQueryClient } from '@tanstack/react-query';

import { dockerService, type DockerInstallOptions } from '../../core/services/docker.service';
import { openExternalUrl } from '../../core/ipc/transport';
import { dockerActionStore } from './dockerActionStore';
import type {
    ContainerInfo,
    DeployedContainer,
    DockerDeploySpec,
    DockerInstallReport,
    DockerStatus,
} from '../../core/ipc/types';

const DOCKER_INSTALL_TIMEOUT_MS = 10 * 60 * 1000;

/// Docker Desktop 下载页，Windows / macOS 走手动安装时引导用。
const DOCKER_DESKTOP_URL = 'https://www.docker.com/products/docker-desktop/';

export interface UseDockerHostsResult {
    /// host_id → 该主机的 docker 探测结果（还没回来时 undefined）。
    statusByHost: Record<string, DockerStatus | undefined>;
    /// host_id → 是否正在探测。
    probingByHost: Record<string, boolean>;
    /// host_id → 该主机已有的容器列表（daemon 就绪才查，否则空数组）。
    /// 框架行的「Docker 部署」按钮据此判定同 flavor 是否已部署。
    containersByHost: Record<string, ContainerInfo[]>;
    /// 刷新所有主机的 docker 探测。
    refetch: () => void;
    /// 在某主机上装 / 起 docker。返回结构化结果,调用方按 status 分流(可能需要
    /// 弹框要 sudo 密码后带 options 重试)。
    install: (hostId: string, options?: DockerInstallOptions) => Promise<DockerInstallReport>;
    /// 任一主机正在装 docker(全局布尔,兼容旧调用方)。精确到某台用 installingByHost。
    isInstalling: boolean;
    /// host_id → 该主机是否正在装 docker。状态存模块级 store,切页面不丢。
    installingByHost: Record<string, boolean>;
    /// 打开 Docker Desktop 下载页（Windows / macOS 手动安装引导用）。
    openDownloadPage: () => Promise<void>;
    /// 在某主机上部署一个容器。taskId 由调用方生成并传入，用于订阅进度事件。
    deploy: (hostId: string, spec: DockerDeploySpec, taskId: string) => Promise<DeployedContainer>;
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

    // 容器列表：仅对 daemon 就绪的主机查（没起 daemon 时列容器必失败，禁用避免反复报错）。
    // 给框架行「Docker 部署」按钮判定同 flavor 是否已部署用。
    const containerQueries = useQueries({
        queries: hostIds.map((hostId) => ({
            queryKey: ['docker', 'containers', hostId],
            queryFn: () => dockerService.listContainers(hostId),
            enabled: statusByHost[hostId]?.daemonRunning ?? false,
            staleTime: 30 * 1000,
        })),
    });

    const containersByHost = useMemo<Record<string, ContainerInfo[]>>(() => {
        const out: Record<string, ContainerInfo[]> = {};
        hostIds.forEach((id, i) => {
            out[id] = containerQueries[i]?.data ?? [];
        });
        return out;
    }, [hostIds, containerQueries]);

    const invalidate = useCallback(() => {
        queryClient.invalidateQueries({ queryKey: ['docker'] });
        queryClient.invalidateQueries({ queryKey: ['docker', 'containers'] });
    }, [queryClient]);

    const installMutation = useMutation({
        mutationFn: (args: { hostId: string; options?: DockerInstallOptions }) =>
            dockerService.install(args.hostId, args.options),
        onSuccess: invalidate,
    });

    // 订阅模块级 docker 安装状态(切页面不丢)。useMutation.isPending 绑在 hook 上,
    // 页面卸载即清,所以"正在装"的真相存在 dockerActionStore 里。
    const installingByHost = useSyncExternalStore(
        dockerActionStore.subscribe,
        () => dockerActionStore.getSnapshot().installingByHost,
    );

    // install 包一层:进 store 标记 installing,promise 落定(成败都算)清标记。
    // 这样切页面再切回,spinner / 禁用态从 store 恢复,而不是凭空消失。
    const install = useCallback(
        async (hostId: string, options?: DockerInstallOptions): Promise<DockerInstallReport> => {
            dockerActionStore.markInstalling(hostId);
            try {
                const work = installMutation.mutateAsync({ hostId, options });
                const timeout = new Promise<never>((_, reject) => {
                    window.setTimeout(
                        () =>
                            reject(
                                new Error(
                                    'Docker 安装超时（10 分钟）。请检查 SSH 与远端网络，或在远端手动安装后点刷新。',
                                ),
                            ),
                        DOCKER_INSTALL_TIMEOUT_MS,
                    );
                });
                return await Promise.race([work, timeout]);
            } finally {
                dockerActionStore.clearInstalling(hostId);
            }
        },
        [installMutation],
    );

    const isInstalling = Object.keys(installingByHost).length > 0;

    const deployMutation = useMutation({
        mutationFn: (args: { hostId: string; spec: DockerDeploySpec; taskId: string }) =>
            dockerService.deploy(args.hostId, args.spec, args.taskId),
        onSuccess: invalidate,
    });

    return {
        statusByHost,
        probingByHost,
        containersByHost,
        refetch: invalidate,
        install,
        isInstalling,
        installingByHost,
        openDownloadPage: () => openExternalUrl(DOCKER_DESKTOP_URL),
        deploy: (hostId, spec, taskId) => deployMutation.mutateAsync({ hostId, spec, taskId }),
        isDeploying: deployMutation.isPending,
    };
}
