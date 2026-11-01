// Docker 管理面 IPC 服务层。
// 唯一持有 docker_* Tauri command 名字符串的位置，后端改命令名只用改这一处。
// 部署 / 容器管理都按 host_id（"local" 或 "remote:<id>"）选主机。

import type { ContainerEngine } from '../ipc/generated/domain/ContainerEngine';
import { invoke, isTauri } from '../ipc/transport';
import type {
    ContainerAction,
    ContainerInfo,
    DeployedContainer,
    DockerFlavor,
    DockerInstallReport,
    DockerStatus,
    DockerContainerDetails,
    DockerContainerStats,
    DockerNetworkInfo,
    DockerVolumeInfo,
    DockerResourceKind,
    ImageInfo,
} from '../ipc/types';
import {
    mockDockerStatus,
    mockDeployed,
    mockProgressSequence,
    withMockDelay,
    mockContainerEngine,
} from '../ipc/mock/docker.mock';
import {
    mockDockerHost,
    mockContainerDetails,
    mockContainerAction,
} from '../ipc/mock/docker-management.mock';

/// 安装 docker 的可选入参。sudoPassword 是用户在弹框输入的 sudo 密码,
/// rememberSudo 是用户勾的"记住密码"。两者都不传时后端自动从 keyring 找缓存密码。
export interface DockerInstallOptions {
    sudoPassword?: string;
    rememberSudo?: boolean;
}

export const dockerService = {
    getEngine: async (hostId: string): Promise<ContainerEngine> =>
        isTauri ? invoke('container_engine_get', { hostId }) : mockContainerEngine.get(hostId),
    setEngine: async (hostId: string, engine: ContainerEngine): Promise<void> => {
        if (isTauri) await invoke('container_engine_set', { hostId, engine });
        else mockContainerEngine.set(hostId, engine);
    },
    probe: async (hostId: string, engine?: ContainerEngine): Promise<DockerStatus> => {
        if (isTauri) return invoke<DockerStatus>('docker_probe', { hostId, engine });
        return withMockDelay(
            (engine ?? mockContainerEngine.get(hostId)) === 'podman'
                ? { ...mockDockerStatus, version: '5.8.9' }
                : mockDockerStatus,
            200,
        );
    },

    install: async (
        hostId: string,
        taskId: string,
        options: DockerInstallOptions = {},
        engine?: ContainerEngine,
    ): Promise<DockerInstallReport> => {
        if (isTauri)
            return invoke<DockerInstallReport>('docker_install', {
                hostId,
                taskId,
                engine,
                sudoPassword: options.sudoPassword ?? null,
                rememberSudo: options.rememberSudo ?? null,
            });
        return withMockDelay(
            {
                status: 'installed',
                message: 'Docker 安装完成（mock）',
            } satisfies DockerInstallReport,
            400,
        );
    },

    listContainers: async (hostId: string, engine?: ContainerEngine): Promise<ContainerInfo[]> => {
        if (isTauri) return invoke<ContainerInfo[]>('docker_list_containers', { hostId, engine });
        return withMockDelay(
            [...mockDockerHost(engine ? `${hostId}::${engine}` : hostId).containers],
            200,
        );
    },

    listImages: async (hostId: string, engine?: ContainerEngine): Promise<ImageInfo[]> => {
        if (isTauri) return invoke<ImageInfo[]>('docker_list_images', { hostId, engine });
        return withMockDelay(
            [...mockDockerHost(engine ? `${hostId}::${engine}` : hostId).images],
            200,
        );
    },

    removeImage: async (
        hostId: string,
        imageRef: string,
        force?: boolean,
        engine?: ContainerEngine,
    ): Promise<void> => {
        if (isTauri)
            return invoke<void>('docker_remove_image', {
                hostId,
                imageRef,
                force: force ?? null,
                engine,
            });
        const host = mockDockerHost(engine ? `${hostId}::${engine}` : hostId);
        host.images = host.images.filter(
            (i) => `${i.repository}:${i.tag}` !== imageRef && i.id !== imageRef,
        );
        return withMockDelay(undefined, 150);
    },

    containerAction: async (
        hostId: string,
        name: string,
        action: ContainerAction,
        force = false,
        engine?: ContainerEngine,
    ): Promise<void> => {
        if (isTauri)
            return invoke<void>('docker_container_action', { hostId, name, action, force, engine });
        mockContainerAction(engine ? `${hostId}::${engine}` : hostId, name, action, force);
        return withMockDelay(undefined, 150);
    },

    containerDetails: async (
        hostId: string,
        id: string,
        engine?: ContainerEngine,
    ): Promise<DockerContainerDetails> => {
        if (isTauri) return invoke('docker_container_details', { hostId, id, engine });
        return withMockDelay(
            mockContainerDetails(engine ? `${hostId}::${engine}` : hostId, id),
            150,
        );
    },

    containerStats: async (
        hostId: string,
        id: string,
        engine?: ContainerEngine,
    ): Promise<DockerContainerStats> => {
        if (isTauri) return invoke('docker_container_stats', { hostId, id, engine });
        return withMockDelay(
            {
                cpuPercent: '1.24%',
                memoryUsage: '128MiB / 2GiB',
                memoryPercent: '6.25%',
                networkIo: '12MB / 4MB',
                blockIo: '28MB / 2MB',
                pids: '18',
            },
            150,
        );
    },

    listNetworks: async (
        hostId: string,
        engine?: ContainerEngine,
    ): Promise<DockerNetworkInfo[]> => {
        if (isTauri) return invoke('docker_list_networks', { hostId, engine });
        return withMockDelay(
            [...mockDockerHost(engine ? `${hostId}::${engine}` : hostId).networks],
            150,
        );
    },

    listVolumes: async (hostId: string, engine?: ContainerEngine): Promise<DockerVolumeInfo[]> => {
        if (isTauri) return invoke('docker_list_volumes', { hostId, engine });
        return withMockDelay(
            [...mockDockerHost(engine ? `${hostId}::${engine}` : hostId).volumes],
            150,
        );
    },

    removeResource: async (
        hostId: string,
        kind: DockerResourceKind,
        id: string,
        engine?: ContainerEngine,
    ): Promise<void> => {
        if (isTauri) return invoke('docker_remove_resource', { hostId, kind, id, engine });
        const host = mockDockerHost(engine ? `${hostId}::${engine}` : hostId);
        if (kind === 'network') host.networks = host.networks.filter((n) => n.id !== id);
        else host.volumes = host.volumes.filter((v) => v.name !== id);
        return withMockDelay(undefined, 150);
    },

    logs: async (
        hostId: string,
        name: string,
        tail: number,
        engine?: ContainerEngine,
    ): Promise<string> => {
        if (isTauri) return invoke<string>('docker_logs', { hostId, name, tail, engine });
        return withMockDelay(`mock logs for ${name}\nline 1\nline 2`, 150);
    },

    imageReadyForFlavor: async (
        hostId: string,
        flavor: DockerFlavor,
        engine?: ContainerEngine,
    ): Promise<boolean> => {
        if (isTauri)
            return invoke<boolean>('docker_image_ready_for_flavor', { hostId, flavor, engine });
        return withMockDelay(true, 100);
    },

    pullFrameworkImage: async (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
        engine?: ContainerEngine,
    ): Promise<DeployedContainer> => {
        const spec = {
            flavor,
            ...(mirror && mirror.trim() ? { mirror: mirror.trim() } : {}),
        };
        if (isTauri)
            return invoke<DeployedContainer>('docker_deploy', { hostId, spec, taskId, engine });
        mockProgressSequence(taskId);
        return withMockDelay(mockDeployed(flavor), 2000);
    },

    composeDown: async (
        hostId: string,
        name: string,
        removeVolumes: boolean,
        engine?: ContainerEngine,
    ): Promise<void> => {
        if (isTauri)
            return invoke<void>('docker_compose_down', { hostId, name, removeVolumes, engine });
        return withMockDelay(undefined, 200);
    },
};
