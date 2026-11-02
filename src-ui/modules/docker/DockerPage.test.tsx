// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DockerHostPage } from './DockerPage.next';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { dockerActionStore } from '../../hooks/docker/dockerActionStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import type { ContainerInfo, DockerContainerDetails } from '../../core/ipc/types';

const service = vi.hoisted(() => ({
    getEngine: vi.fn(),
    setEngine: vi.fn(),
    probe: vi.fn(),
    listContainers: vi.fn(),
    listImages: vi.fn(),
    listNetworks: vi.fn(),
    listVolumes: vi.fn(),
    containerAction: vi.fn(),
    removeImage: vi.fn(),
    removeResource: vi.fn(),
    containerDetails: vi.fn(),
    containerStats: vi.fn(),
    logs: vi.fn(),
}));
vi.mock('../../core/services/docker.service', () => ({ dockerService: service }));
vi.mock('../../hooks/remote/useServerManager', () => ({
    useServerManager: () => ({ servers: [] }),
}));
vi.mock('../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));
vi.mock('../../hooks/ui/globalInfoBarStore', () => ({
    pushInfoBar: vi.fn(),
    dismissInfoBar: vi.fn(),
}));
vi.mock('../../core/domain/runtime/env', async (original) => ({
    ...(await original<typeof import('../../core/domain/runtime/env')>()),
    localOs: () => 'linux',
}));

let client: QueryClient;
const container = (engine: string): ContainerInfo => ({
    id: 'same-id',
    name: `${engine}-container`,
    image: 'demo:latest',
    state: 'running',
    status: 'Up',
    ports: [],
});
const details: DockerContainerDetails = {
    ...container('docker'),
    created: '',
    startedAt: '',
    finishedAt: '',
    exitCode: 0,
    oomKilled: false,
    error: '',
    restartCount: 0,
    restartPolicy: 'no',
    health: null,
    networkMode: 'bridge',
    networks: [],
    mounts: [],
};
function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((yes, no) => {
        resolve = yes;
        reject = no;
    });
    return { promise, resolve, reject };
}
function mount() {
    return render(
        <QueryClientProvider client={client}>
            <DockerHostPage hostId="local" servers={[]} onChangeHost={vi.fn()} />
        </QueryClientProvider>,
    );
}
beforeEach(() => {
    vi.clearAllMocks();
    Object.values(service).forEach((mock) => mock.mockReset());
    preferencesStore.setMotionEnabled(false);
    dockerActionStore._reset();
    client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
    service.getEngine.mockResolvedValue('docker');
    service.setEngine.mockResolvedValue(undefined);
    service.probe.mockResolvedValue({
        installed: true,
        daemonRunning: true,
        composeAvailable: true,
        version: '1',
    });
    service.listContainers.mockImplementation((_host, engine) =>
        Promise.resolve([container(engine)]),
    );
    service.listImages.mockResolvedValue([
        { id: 'image', repository: 'demo', tag: 'latest', size: '1MB', createdSince: 'today' },
    ]);
    service.listNetworks.mockResolvedValue([]);
    service.listVolumes.mockResolvedValue([]);
    service.containerDetails.mockResolvedValue(details);
    service.containerStats.mockResolvedValue({
        cpuPercent: '1%',
        memoryUsage: '1MB',
        memoryPercent: '1%',
        networkIo: '0',
        blockIo: '0',
        pids: '1',
    });
    service.logs.mockResolvedValue('example log');
});
afterEach(() => {
    cleanup();
    client.clear();
    dockerActionStore._reset();
    preferencesStore.reset();
});

describe('容器管理引擎边界', () => {
    it('不显示自动刷新开关，保留手动刷新和当前筛选', async () => {
        mount();
        await screen.findByText('docker-container');
        expect(screen.queryByRole('checkbox', { name: '自动刷新' })).not.toBeInTheDocument();
        const search = screen.getByRole('textbox', { name: '搜索容器' });
        fireEvent.change(search, { target: { value: 'docker' } });
        const before = service.listContainers.mock.calls.length;
        fireEvent.click(screen.getByRole('button', { name: '刷新', exact: true }));
        await waitFor(() =>
            expect(service.listContainers.mock.calls.length).toBeGreaterThan(before),
        );
        expect(search).toHaveValue('docker');
        expect(screen.getByRole('button', { name: '容器部署', exact: true })).toBeInTheDocument();
    });

    it('容器操作进行中不能切换引擎', async () => {
        const operation = deferred<void>();
        service.containerAction.mockReturnValueOnce(operation.promise);
        mount();
        await screen.findByText('docker-container');
        fireEvent.click(screen.getByRole('button', { name: '停止', exact: true }));
        await waitFor(() => expect(screen.getByRole('radio', { name: 'Podman' })).toBeDisabled());
        fireEvent.click(screen.getByRole('radio', { name: 'Podman' }));
        expect(service.setEngine).not.toHaveBeenCalled();
        await act(async () => operation.resolve());
        await waitFor(() => expect(screen.getByRole('radio', { name: 'Podman' })).toBeEnabled());
    });

    it('跨页拉镜像期间锁定同主机切换', async () => {
        dockerActionStore.markPulling('local', 'snowluma', 'pull-task');
        mount();
        await screen.findByText('docker-container');
        expect(screen.getByRole('radio', { name: 'Podman' })).toBeDisabled();
        act(() => dockerActionStore.clearPulling('local', 'snowluma'));
        await waitFor(() => expect(screen.getByRole('radio', { name: 'Podman' })).toBeEnabled());
    });

    it.each([
        ['containerDetails', '详情', '读取容器详情失败'],
        ['containerStats', '详情', '读取资源占用失败'],
        ['logs', '查看容器 docker-container 的运行日志', '读取容器日志失败'],
    ] as const)('%s 失败通过 InfoBar 展示，不塞入弹窗', async (method, button, title) => {
        service[method].mockRejectedValue(new Error('read denied'));
        mount();
        await screen.findByText('docker-container');
        fireEvent.click(screen.getByRole('button', { name: button, exact: true }));
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ title, raw: 'read denied' }),
            ),
        );
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
        expect(
            within(screen.getByRole('dialog')).queryByText('read denied'),
        ).not.toBeInTheDocument();
    });

    it('删除失败只发一次 InfoBar，保留确认弹窗', async () => {
        service.containerAction.mockRejectedValueOnce(new Error('container busy'));
        mount();
        await screen.findByText('docker-container');
        fireEvent.click(screen.getByRole('button', { name: '删除', exact: true }));
        const dialog = await screen.findByRole('dialog');
        fireEvent.click(within(dialog).getByRole('button', { name: '删除容器' }));
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ title: '容器删除失败' }),
            ),
        );
        expect(pushErrorBar).toHaveBeenCalledTimes(1);
        expect(screen.getByRole('dialog')).toBeInTheDocument();
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });

    it('读取引擎失败不回退 Docker，重试后只查询实际引擎', async () => {
        service.getEngine.mockRejectedValueOnce(new Error('SSH unavailable'));
        mount();
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ title: '读取容器引擎失败' }),
            ),
        );
        expect(service.probe).not.toHaveBeenCalled();
        expect(service.listContainers).not.toHaveBeenCalled();
        expect(screen.getByRole('radio', { name: 'Docker' })).not.toBeChecked();
        expect(screen.getByRole('radio', { name: 'Podman' })).not.toBeChecked();
        expect(screen.getByRole('radio', { name: 'Docker' })).toBeDisabled();
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
        service.getEngine.mockResolvedValue('podman');
        fireEvent.click(screen.getByRole('button', { name: '重试' }));
        await screen.findByText('podman-container');
        expect(service.probe.mock.calls.every(([, engine]) => engine === 'podman')).toBe(true);
    });

    it('持久化切换完成前不请求新引擎，完成后只对新引擎操作', async () => {
        const saving = deferred<void>();
        service.setEngine.mockReturnValueOnce(saving.promise);
        mount();
        await screen.findByText('docker-container');
        fireEvent.click(screen.getByRole('radio', { name: 'Podman' }));
        await waitFor(() => expect(service.setEngine).toHaveBeenCalledWith('local', 'podman'));
        expect(screen.queryByText('docker-container')).not.toBeInTheDocument();
        expect(service.probe).not.toHaveBeenCalledWith('local', 'podman');
        expect(screen.getByRole('radio', { name: 'Docker' })).toBeDisabled();
        await act(async () => saving.resolve());
        await screen.findByText('podman-container');
        fireEvent.click(screen.getByRole('button', { name: '停止', exact: true }));
        await waitFor(() =>
            expect(service.containerAction).toHaveBeenCalledWith(
                'local',
                'same-id',
                'stop',
                undefined,
                'podman',
            ),
        );
    });

    it('切换失败保留原引擎，错误只发一次 InfoBar', async () => {
        service.setEngine.mockRejectedValueOnce(new Error('cannot persist'));
        mount();
        await screen.findByText('docker-container');
        fireEvent.click(screen.getByRole('radio', { name: 'Podman' }));
        await waitFor(() =>
            expect(pushErrorBar).toHaveBeenCalledWith(
                expect.objectContaining({ title: '无法切换容器引擎' }),
            ),
        );
        await screen.findByText('docker-container');
        expect(screen.getByRole('radio', { name: 'Docker' })).toBeChecked();
        expect(service.probe).not.toHaveBeenCalledWith('local', 'podman');
        expect(pushErrorBar).toHaveBeenCalledTimes(1);
    });

    it.each(['详情', '日志', '删除容器', '删除镜像'])('引擎变更关闭旧%s弹窗', async (kind) => {
        mount();
        await screen.findByText('docker-container');
        if (kind === '删除镜像') {
            fireEvent.click(screen.getByRole('button', { name: '镜像', exact: true }));
            await screen.findByText('demo:latest', { selector: 'h3' });
        }
        fireEvent.click(
            screen.getByRole('button', {
                name:
                    kind === '日志'
                        ? '查看容器 docker-container 的运行日志'
                        : kind.startsWith('删除')
                          ? '删除'
                          : kind,
                exact: true,
            }),
        );
        await screen.findByRole('dialog');
        await act(async () => {
            client.setQueryData(['docker', 'engine', 'local'], 'podman');
        });
        if (kind === '删除镜像') {
            await waitFor(() => expect(service.listImages).toHaveBeenCalledWith('local', 'podman'));
            expect(screen.getByRole('button', { name: '镜像', exact: true })).toHaveAttribute(
                'aria-pressed',
                'true',
            );
        } else {
            await screen.findByText('podman-container');
        }
        expect(screen.queryByRole('dialog', { hidden: true })).not.toBeInTheDocument();
        expect(service.containerAction).not.toHaveBeenCalled();
        expect(service.removeImage).not.toHaveBeenCalled();
        expect(service.containerDetails).not.toHaveBeenCalledWith('local', 'same-id', 'podman');
        expect(service.logs).not.toHaveBeenCalledWith(
            'local',
            'same-id',
            expect.anything(),
            'podman',
        );
    });
});
