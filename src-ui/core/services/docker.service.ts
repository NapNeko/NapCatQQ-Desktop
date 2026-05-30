// Docker 管理面 IPC 服务层。
// 唯一持有 docker_* Tauri command 名字符串的位置（R3：单一字面量来源）。
// 部署 / 容器管理都按 host_id（"local" 或 "remote:<id>"）选主机。

import { invoke, isTauri } from '../ipc/transport';
import type {
    ContainerAction,
    ContainerInfo,
    DeployedContainer,
    DockerDeploySpec,
    DockerStatus,
} from '../ipc/types';
import {
    mockDockerStatus,
    mockContainers,
    mockDeployed,
    withMockDelay,
} from '../ipc/mock/docker.mock';

export const dockerService = {
    probe: async (hostId: string): Promise<DockerStatus> => {
        if (isTauri) return invoke<DockerStatus>('docker_probe', { hostId });
        return withMockDelay(mockDockerStatus, 200);
    },

    install: async (hostId: string): Promise<string> => {
        if (isTauri) return invoke<string>('docker_install', { hostId });
        return withMockDelay('docker 已就绪（mock）', 400);
    },

    listContainers: async (hostId: string): Promise<ContainerInfo[]> => {
        if (isTauri) return invoke<ContainerInfo[]>('docker_list_containers', { hostId });
        return withMockDelay(mockContainers, 200);
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

    deploy: async (hostId: string, spec: DockerDeploySpec): Promise<DeployedContainer> => {
        if (isTauri) return invoke<DeployedContainer>('docker_deploy', { hostId, spec });
        return withMockDelay(mockDeployed(spec), 600);
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
