// Docker 管理页预览数据按主机隔离。
import type {
    ContainerAction,
    DockerContainerDetails,
    DockerNetworkInfo,
    DockerVolumeInfo,
} from '../types';
import { mockContainers, mockImages } from './docker.mock';

function createHost() {
    return {
        containers: structuredClone(mockContainers),
        images: structuredClone(mockImages),
        networks: [
            { id: 'network-bridge', name: 'bridge', driver: 'bridge', scope: 'local' },
            { id: 'network-napcat', name: 'napcat_default', driver: 'bridge', scope: 'local' },
        ] satisfies DockerNetworkInfo[],
        volumes: [
            { name: 'napcat_data', driver: 'local', scope: 'local' },
            { name: 'old_cache', driver: 'local', scope: 'local' },
        ] satisfies DockerVolumeInfo[],
    };
}

const hosts = new Map<string, ReturnType<typeof createHost>>();
export function mockDockerHost(hostId: string) {
    let host = hosts.get(hostId);
    if (!host) {
        host = createHost();
        hosts.set(hostId, host);
    }
    return host;
}

export function mockContainerDetails(hostId: string, id: string): DockerContainerDetails {
    const container = mockDockerHost(hostId).containers.find((c) => c.id === id || c.name === id);
    if (!container) throw new Error('容器已不存在');
    return {
        ...container,
        created: '2026-10-12T08:00:00Z',
        startedAt: '2026-10-13T01:00:00Z',
        finishedAt: '',
        exitCode: 0,
        oomKilled: false,
        error: '',
        restartCount: 0,
        restartPolicy: 'unless-stopped',
        health: 'healthy',
        networkMode: 'bridge',
        networks: ['napcat_default'],
        mounts: [
            {
                kind: 'volume',
                source: 'napcat_data',
                destination: '/app/napcat/config',
                readOnly: false,
            },
        ],
    };
}

export function mockContainerAction(
    hostId: string,
    id: string,
    action: ContainerAction,
    force: boolean,
) {
    const host = mockDockerHost(hostId);
    const container = host.containers.find((c) => c.id === id || c.name === id);
    if (!container) throw new Error('容器已不存在');
    if (action === 'remove') {
        if (['running', 'paused', 'restarting'].includes(container.state) && !force) {
            throw new Error('容器正在运行，请先停止或明确选择强制删除');
        }
        host.containers = host.containers.filter((c) => c.id !== container.id);
    } else {
        container.state = action === 'stop' ? 'exited' : action === 'pause' ? 'paused' : 'running';
        container.status = container.state === 'running' ? 'Up 1 second' : container.state;
    }
}
