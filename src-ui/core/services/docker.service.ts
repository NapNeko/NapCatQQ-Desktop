// Docker 管理面 IPC 服务层。
// 唯一持有 docker_* Tauri command 名字符串的位置（R3：单一字面量来源）。
// 部署 / 容器管理都按 host_id（"local" 或 "remote:<id>"）选主机。

import { invoke, isTauri } from '../ipc/transport';
import type {
    ContainerAction,
    ContainerInfo,
    DeployedContainer,
    DockerFlavor,
    DockerInstallReport,
    DockerStatus,
    ImageInfo,
} from '../ipc/types';
import {
    mockDockerStatus,
    mockContainers,
    mockDeployed,
    mockImages,
    mockProgressSequence,
    withMockDelay,
} from '../ipc/mock/docker.mock';

/// 安装 docker 的可选入参。sudoPassword 是用户在弹框输入的 sudo 密码,
/// rememberSudo 是用户勾的"记住密码"。两者都不传时后端自动从 keyring 找缓存密码。
export interface DockerInstallOptions {
    sudoPassword?: string;
    rememberSudo?: boolean;
}

export const dockerService = {
    probe: async (hostId: string): Promise<DockerStatus> => {
        if (isTauri) return invoke<DockerStatus>('docker_probe', { hostId });
        return withMockDelay(mockDockerStatus, 200);
    },

    install: async (
        hostId: string,
        taskId: string,
        options: DockerInstallOptions = {},
    ): Promise<DockerInstallReport> => {
        if (isTauri)
            return invoke<DockerInstallReport>('docker_install', {
                hostId,
                taskId,
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

    listContainers: async (hostId: string): Promise<ContainerInfo[]> => {
        if (isTauri) return invoke<ContainerInfo[]>('docker_list_containers', { hostId });
        return withMockDelay(mockContainers, 200);
    },

    listImages: async (hostId: string): Promise<ImageInfo[]> => {
        if (isTauri) return invoke<ImageInfo[]>('docker_list_images', { hostId });
        return withMockDelay(mockImages, 200);
    },

    removeImage: async (hostId: string, imageRef: string, force?: boolean): Promise<void> => {
        if (isTauri)
            return invoke<void>('docker_remove_image', {
                hostId,
                imageRef,
                force: force ?? null,
            });
        return withMockDelay(undefined, 150);
    },

    containerAction: async (
        hostId: string,
        name: string,
        action: ContainerAction,
    ): Promise<void> => {
        if (isTauri) return invoke<void>('docker_container_action', { hostId, name, action });
        return withMockDelay(undefined, 150);
    },

    logs: async (hostId: string, name: string, tail: number): Promise<string> => {
        if (isTauri) return invoke<string>('docker_logs', { hostId, name, tail });
        return withMockDelay(`mock logs for ${name}\nline 1\nline 2`, 150);
    },

    imageReadyForFlavor: async (hostId: string, flavor: DockerFlavor): Promise<boolean> => {
        if (isTauri)
            return invoke<boolean>('docker_image_ready_for_flavor', { hostId, flavor });
        return withMockDelay(true, 100);
    },

    pullFrameworkImage: async (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
    ): Promise<DeployedContainer> => {
        const spec = {
            flavor,
            ...(mirror && mirror.trim() ? { mirror: mirror.trim() } : {}),
        };
        if (isTauri) return invoke<DeployedContainer>('docker_deploy', { hostId, spec, taskId });
        mockProgressSequence(taskId);
        return withMockDelay(mockDeployed(flavor), 2000);
    },

    composeDown: async (
        hostId: string,
        name: string,
        removeVolumes: boolean,
    ): Promise<void> => {
        if (isTauri) return invoke<void>('docker_compose_down', { hostId, name, removeVolumes });
        return withMockDelay(undefined, 200);
    },
};
