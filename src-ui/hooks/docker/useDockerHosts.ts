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
import { dockerActionStore, dockerPullTargetKey } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import type {
    ContainerInfo,
    DeployedContainer,
    DockerFlavor,
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
    /// host_id → 该主机 NapCat/SnowLuma 官方镜像是否已在本地（组件页「已拉取」判定）。
    imageReadyByHost: Record<string, Partial<Record<DockerFlavor, boolean | undefined>>>;
    /// host_id → 容器列表（Docker 管理页等）。
    containersByHost: Record<string, ContainerInfo[]>;
    refetch: () => void;
    /// 在某主机上装 / 起 docker。返回结构化结果,调用方按 status 分流(可能需要
    /// 弹框要 sudo 密码后带 options 重试)。
    install: (hostId: string, options?: DockerInstallOptions) => Promise<DockerInstallReport>;
    /// 任一主机正在装 docker(全局布尔,兼容旧调用方)。精确到某台用 installingByHost。
    isInstalling: boolean;
    /// host_id → 该主机是否正在装 docker。状态存模块级 store,切页面不丢。
    installingByHost: Record<string, boolean>;
    /// 安装中时 Docker 行展示的说明（无事件订阅时的兜底文案）。
    installHintByHost: Record<string, string>;
    /// 打开 Docker Desktop 下载页（Windows / macOS 手动安装引导用）。
    openDownloadPage: () => Promise<void>;
    /// 在远端拉取 NapCat/SnowLuma 框架镜像（不创建容器）。
    /// mirror: auto 省略；hub 仅官方；或镜像站主机名如 docker.1ms.run
    pullFrameworkImage: (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
    ) => Promise<DeployedContainer>;
    /** 该主机该口味是否正在拉镜像（模块级 store，切页不丢） */
    isPullingFrameworkImage: (hostId: string, flavor: DockerFlavor) => boolean;
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

    const imageReadyNapcatQueries = useQueries({
        queries: hostIds.map((hostId) => ({
            queryKey: ['docker', 'imageReady', hostId, 'napcat'],
            queryFn: () => dockerService.imageReadyForFlavor(hostId, 'napcat'),
            enabled: statusByHost[hostId]?.daemonRunning ?? false,
            staleTime: 30 * 1000,
        })),
    });

    const imageReadySnowlumaQueries = useQueries({
        queries: hostIds.map((hostId) => ({
            queryKey: ['docker', 'imageReady', hostId, 'snowluma'],
            queryFn: () => dockerService.imageReadyForFlavor(hostId, 'snowluma'),
            enabled: statusByHost[hostId]?.daemonRunning ?? false,
            staleTime: 30 * 1000,
        })),
    });

    const imageReadyByHost = useMemo<
        Record<string, Partial<Record<DockerFlavor, boolean | undefined>>>
    >(() => {
        const out: Record<string, Partial<Record<DockerFlavor, boolean | undefined>>> = {};
        hostIds.forEach((id, i) => {
            out[id] = {
                napcat: imageReadyNapcatQueries[i]?.data,
                snowluma: imageReadySnowlumaQueries[i]?.data,
            };
        });
        return out;
    }, [hostIds, imageReadyNapcatQueries, imageReadySnowlumaQueries]);

    // 容器列表：Docker 管理页等仍需要；框架「拉镜像」不再依赖容器是否存在。
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
        queryClient.invalidateQueries({ queryKey: ['docker', 'imageReady'] });
    }, [queryClient]);

    const installMutation = useMutation({
        mutationFn: (args: { hostId: string; taskId: string; options?: DockerInstallOptions }) =>
            dockerService.install(args.hostId, args.taskId, args.options),
        onSuccess: (report, args) => {
            if (report.probedStatus) {
                queryClient.setQueryData(['docker', 'status', args.hostId], report.probedStatus);
            }
            invalidate();
        },
    });

    // 订阅模块级 docker 安装状态(切页面不丢)。useMutation.isPending 绑在 hook 上,
    // 页面卸载即清,所以"正在装"的真相存在 dockerActionStore 里。
    const installingByHost = useSyncExternalStore(
        dockerActionStore.subscribe,
        () => dockerActionStore.getSnapshot().installingByHost,
    );
    const installHintByHost = useSyncExternalStore(
        dockerActionStore.subscribe,
        () => dockerActionStore.getSnapshot().installHintByHost,
    );

    const pullingByTarget = useSyncExternalStore(
        dockerActionStore.subscribe,
        () => dockerActionStore.getSnapshot().pullingByTarget,
    );

    // install 包一层:进 store 标记 installing。清理由 useDockerInstallProgressBridge
    // 监听终态事件统一处理，这样状态与进度事件严格同步。
    // 异常情况（超时、网络错误）需手动清理。
    const install = useCallback(
        async (hostId: string, options?: DockerInstallOptions): Promise<DockerInstallReport> => {
            const taskId = crypto.randomUUID();
            dockerInstallProgressStore.started(taskId, hostId);
            dockerActionStore.markInstalling(hostId, taskId);
            try {
                const work = installMutation.mutateAsync({ hostId, taskId, options });
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
            } catch (err) {
                // 超时或网络异常：后端未发送终态事件，需手动清理
                dockerActionStore.clearInstalling(hostId);
                throw err;
            }
        },
        [installMutation],
    );

    const isInstalling = Object.keys(installingByHost).length > 0;

    const pullMutation = useMutation({
        mutationFn: (args: {
            hostId: string;
            flavor: DockerFlavor;
            taskId: string;
            mirror?: string | null;
        }) =>
            dockerService.pullFrameworkImage(
                args.hostId,
                args.flavor,
                args.taskId,
                args.mirror,
            ),
        onSuccess: (_result, args) => {
            queryClient.setQueryData(
                ['docker', 'imageReady', args.hostId, args.flavor],
                true,
            );
            invalidate();
        },
    });

    const pullFrameworkImage = useCallback(
        async (
            hostId: string,
            flavor: DockerFlavor,
            taskId: string,
            mirror?: string | null,
        ): Promise<DeployedContainer> => {
            const key = dockerPullTargetKey(hostId, flavor);
            const snap = dockerActionStore.getSnapshot();
            const activeTask = snap.pullTaskIdByTarget[key];
            if (activeTask && activeTask !== taskId) {
                throw new Error('该主机正在拉取此框架镜像，请在任务队列查看进度');
            }
            if (!activeTask) {
                dockerActionStore.markPulling(hostId, flavor, taskId);
            }
            try {
                return await pullMutation.mutateAsync({ hostId, flavor, taskId, mirror });
            } catch (err) {
                dockerActionStore.clearPulling(hostId, flavor);
                throw err;
            }
        },
        [pullMutation],
    );

    const isPullingFrameworkImage = useCallback(
        (hostId: string, flavor: DockerFlavor) =>
            !!pullingByTarget[dockerPullTargetKey(hostId, flavor)],
        [pullingByTarget],
    );

    return {
        statusByHost,
        probingByHost,
        imageReadyByHost,
        containersByHost,
        refetch: invalidate,
        install,
        isInstalling,
        installingByHost,
        installHintByHost,
        openDownloadPage: () => openExternalUrl(DOCKER_DESKTOP_URL),
        pullFrameworkImage,
        isPullingFrameworkImage,
        isDeploying: pullMutation.isPending,
    };
}
