// 浏览器预览模式下的 Docker 假数据。
// 真 IPC 实装在 core/services/docker.service.ts。

import type {
    ContainerInfo,
    DeployedContainer,
    DockerDeploySpec,
    DockerStatus,
    ProgressEvent,
} from '../types';
import { emitMockEvent } from './events.mock';

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

// 模拟一组 docker pull 进度事件，让浏览器预览能看到进度动画。
// 走 emitMockEvent 发 docker_deploy_progress DomainEvent，和真实链路一致：
// useDockerDeployProgressBridge 订阅事件流后路由进 store。这样 mock 不直接
// 碰 hooks 层，保持 core → hooks 的分层方向不被倒置。
export function mockProgressSequence(taskId: string): void {
    const ts = () => BigInt(Date.now());
    // 用 unknown 中转：ProgressEvent 是 discriminated union，对象字面量无法直接推断变体。
    const ev = (raw: object): ProgressEvent =>
        ({ v: 1, timestamp_ms: ts(), ...raw } as unknown as ProgressEvent);

    const steps: Array<[number, object]> = [
        [0,    { kind: 'started', total_steps: 3 }],
        [100,  { kind: 'step_begin', step: 1, message: '拉取镜像' }],
        [400,  { kind: 'step_progress', step: 1, percent: 30, message: '拉取镜像层', speed_bps: BigInt(1572864), downloaded_bytes: BigInt(30 * 1024 * 1024), total_bytes: BigInt(100 * 1024 * 1024), download_stage: 'streaming' }],
        [900,  { kind: 'step_progress', step: 1, percent: 70, message: '拉取镜像层', speed_bps: BigInt(2 * 1024 * 1024), downloaded_bytes: BigInt(70 * 1024 * 1024), total_bytes: BigInt(100 * 1024 * 1024), download_stage: 'streaming' }],
        [1300, { kind: 'step_end', step: 1, ok: true }],
        [1400, { kind: 'step_begin', step: 2, message: '创建容器' }],
        [1600, { kind: 'step_progress', step: 2, percent: 50, message: '配置网络', download_stage: null }],
        [1800, { kind: 'step_end', step: 2, ok: true }],
        [1850, { kind: 'step_begin', step: 3, message: '启动容器' }],
        [1950, { kind: 'step_end', step: 3, ok: true }],
        [2000, { kind: 'finished', ok: true }],
    ];
    for (const [delay, raw] of steps) {
        setTimeout(() => {
            emitMockEvent({ kind: 'docker_deploy_progress', task_id: taskId, event: ev(raw) });
        }, delay);
    }
}
