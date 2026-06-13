// 浏览器预览模式下的 Docker 假数据。
// 真 IPC 实装在 core/services/docker.service.ts。

import type {
    ContainerInfo,
    DeployedContainer,
    DockerDeploySpec,
    DockerStatus,
    ImageInfo,
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

export const mockImages: ImageInfo[] = [
    {
        id: 'deadbeefcafe',
        repository: 'mlikiowa/napcat-docker',
        tag: 'latest',
        size: '1.2GB',
        createdSince: '2 weeks ago',
    },
    {
        id: 'aabbccdd1122',
        repository: 'nginx',
        tag: 'alpine',
        size: '52MB',
        createdSince: '3 months ago',
    },
    {
        id: '112233445566',
        repository: '<none>',
        tag: '<none>',
        size: '128MB',
        createdSince: '5 days ago',
    },
];

/// 拉镜像 mock：返回官方镜像名。
export function mockDeployed(flavor: DockerDeploySpec['flavor']): DeployedContainer {
    const image =
        flavor === 'napcat'
            ? 'mlikiowa/napcat-docker:latest'
            : 'motricseven7/snowluma:latest';
    return { flavor, image };
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
        [0,    { kind: 'started', total_steps: 2 }],
        [100,  { kind: 'step_begin', step: 1, message: '探测 docker 状态' }],
        [200,  { kind: 'step_end', step: 1, ok: true }],
        [300,  { kind: 'step_begin', step: 2, message: '拉取镜像' }],
        [600,  { kind: 'step_progress', step: 2, percent: 12, message: '镜像层 1/3 · 12%', docker_layers: [
            { id: 'aabbccdd1122', phase: '下载中', detail: '[=>       ] 5MB/40MB', done: false },
            { id: '112233445566', phase: '等待', detail: null, done: false },
            { id: 'deadbeefcafe', phase: '完成', detail: null, done: true },
        ]}],
        [1200, { kind: 'step_progress', step: 2, percent: 55, message: '镜像层 2/3 · 55%', docker_layers: [
            { id: 'aabbccdd1122', phase: '解压中', detail: null, done: true },
            { id: '112233445566', phase: '下载中', detail: '[====>    ] 20MB/30MB', done: false },
            { id: 'deadbeefcafe', phase: '完成', detail: null, done: true },
        ]}],
        [1800, { kind: 'step_end', step: 2, ok: true }],
        [2000, { kind: 'finished', ok: true }],
    ];
    for (const [delay, raw] of steps) {
        setTimeout(() => {
            emitMockEvent({ kind: 'docker_deploy_progress', task_id: taskId, event: ev(raw) });
        }, delay);
    }
}
