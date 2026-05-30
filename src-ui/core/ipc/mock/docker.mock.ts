// 浏览器预览模式下的 Docker 假数据。
// 真 IPC 实装在 core/services/docker.service.ts。

import type {
    ContainerInfo,
    DeployedContainer,
    DockerDeploySpec,
    DockerStatus,
} from '../types';

export { withMockDelay } from './bootstrap.mock';

export const mockDockerStatus: DockerStatus = {
    installed: true,
    version: '27.3.1',
    composeAvailable: true,
    daemonRunning: true,
};

export const mockContainers: ContainerInfo[] = [
    {
        id: 'a1b2c3d4e5f6',
        name: 'napcat',
        image: 'mlikiowa/napcat-docker:latest',
        state: 'running',
        status: 'Up 2 hours',
        ports: ['0.0.0.0:6099->6099/tcp', '0.0.0.0:3001->3001/tcp'],
    },
    {
        id: 'f6e5d4c3b2a1',
        name: 'snowluma',
        image: 'motricseven7/snowluma:latest',
        state: 'exited',
        status: 'Exited (0) 10 minutes ago',
        ports: [],
    },
];

/// 部署 mock：按 spec 拼一个看起来合理的回读结果。
export function mockDeployed(spec: DockerDeploySpec): DeployedContainer {
    const port = (container: number, fallback: number) =>
        spec.ports.find((p) => p.container === container)?.host ?? fallback;
    if (spec.flavor === 'napcat') {
        return {
            name: spec.containerName,
            flavor: 'napcat',
            webuiUrl: `http://127.0.0.1:${port(6099, 6099)}/webui`,
            novncUrl: null,
            webuiSecret: 'mock-token-abc123',
        };
    }
    return {
        name: spec.containerName,
        flavor: 'snowluma',
        webuiUrl: `http://127.0.0.1:${port(5099, 5099)}/`,
        novncUrl: `http://127.0.0.1:${port(6081, 6081)}/`,
        webuiSecret: 'mock-vnc-pass',
    };
}
