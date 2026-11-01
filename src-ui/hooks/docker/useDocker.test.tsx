import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useDocker, type DockerTab } from './useDocker';
import { useDockerLogs } from './useDockerContainer';

const service = vi.hoisted(() => ({
    probe: vi.fn(),
    listContainers: vi.fn(),
    listImages: vi.fn(),
    listNetworks: vi.fn(),
    listVolumes: vi.fn(),
    containerAction: vi.fn(),
    logs: vi.fn(),
}));
vi.mock('../../core/services/docker.service', () => ({ dockerService: service }));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: vi.fn() }));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

const clients: QueryClient[] = [];
function setup() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, gcTime: Infinity } },
    });
    clients.push(client);
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { client, wrapper };
}
function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((yes, no) => {
        resolve = yes;
        reject = no;
    });
    return { promise, resolve, reject };
}

beforeEach(() => {
    vi.resetAllMocks();
    service.probe.mockResolvedValue({
        installed: true,
        version: '27',
        daemonRunning: true,
        composeAvailable: false,
    });
    service.listContainers.mockResolvedValue([]);
    service.listImages.mockResolvedValue([]);
    service.listNetworks.mockResolvedValue([]);
    service.listVolumes.mockResolvedValue([]);
});
afterEach(() => {
    cleanup();
    clients.splice(0).forEach((client) => client.clear());
});

describe('Docker 管理查询', () => {
    it('没有主机时不发探测请求', () => {
        renderHook(() => useDocker(''), { wrapper: setup().wrapper });
        expect(service.probe).not.toHaveBeenCalled();
    });
    it('没有 Compose 仍可管理，只加载当前资源页', async () => {
        const { wrapper } = setup();
        const { result, rerender } = renderHook(
            ({ tab }: { tab: DockerTab }) => useDocker('remote:a', tab),
            {
                wrapper,
                initialProps: { tab: 'containers' },
            },
        );
        await waitFor(() => expect(result.current.ready).toBe(true));
        await waitFor(() => expect(service.listContainers).toHaveBeenCalledTimes(1));
        expect(service.listNetworks).not.toHaveBeenCalled();
        rerender({ tab: 'networks' });
        await waitFor(() => expect(service.listNetworks).toHaveBeenCalledWith('remote:a'));
        expect(service.listVolumes).not.toHaveBeenCalled();
    });
    it('列表错误可见，刷新时保持 loading，恢复后清除错误', async () => {
        service.listContainers.mockRejectedValueOnce(new Error('SSH disconnected'));
        const { result } = renderHook(() => useDocker('remote:a'), { wrapper: setup().wrapper });
        await waitFor(() => expect(result.current.listError).toBeTruthy());
        const reload = deferred<[]>();
        service.listContainers.mockReturnValueOnce(reload.promise);
        act(() => result.current.refetch());
        await waitFor(() => expect(result.current.isRefreshing).toBe(true));
        await act(async () => reload.resolve([]));
        await waitFor(() => expect(result.current.listError).toBeNull());
        await waitFor(() => expect(result.current.isRefreshing).toBe(false));
    });
    it('探测失败不读取资源或把缓存当作可操作状态', async () => {
        service.probe.mockRejectedValueOnce(new Error('permission denied'));
        const { result } = renderHook(() => useDocker('remote:a'), { wrapper: setup().wrapper });
        await waitFor(() => expect(result.current.statusError).toBeTruthy());
        expect(result.current.ready).toBe(false);
        expect(service.listContainers).not.toHaveBeenCalled();
    });
    it('切换主机后旧操作只使原主机缓存失效，失败操作不自动重试', async () => {
        const pending = deferred<void>();
        service.containerAction.mockReturnValueOnce(pending.promise);
        const { wrapper, client } = setup();
        const { result, rerender } = renderHook(({ host }) => useDocker(host), {
            wrapper,
            initialProps: { host: 'remote:a' },
        });
        await waitFor(() => expect(result.current.ready).toBe(true));
        act(() => result.current.containerAction({ name: 'container-id', action: 'stop' }));
        await waitFor(() => expect(service.containerAction).toHaveBeenCalledTimes(1));
        rerender({ host: 'remote:b' });
        await waitFor(() => expect(service.listContainers).toHaveBeenCalledWith('remote:b'));
        const before = service.listContainers.mock.calls.filter(
            ([host]) => host === 'remote:b',
        ).length;
        await act(async () => pending.reject(new Error('timeout')));
        await waitFor(() =>
            expect(client.getQueryState(['docker', 'containers', 'remote:a'])?.isInvalidated).toBe(
                true,
            ),
        );
        expect(
            service.listContainers.mock.calls.filter(([host]) => host === 'remote:b'),
        ).toHaveLength(before);
        expect(service.containerAction).toHaveBeenCalledTimes(1);
        expect(service.containerAction).toHaveBeenCalledWith(
            'remote:a',
            'container-id',
            'stop',
            undefined,
            undefined,
        );
    });
    it('迟到的日志响应不会覆盖另一台主机的同名容器', async () => {
        const pending = deferred<string>();
        service.logs.mockImplementation((host: string) =>
            host === 'remote:a' ? pending.promise : Promise.resolve('host B'),
        );
        const { result, rerender } = renderHook(
            ({ host }) => useDockerLogs(host, 'same-id', 400, false),
            {
                wrapper: setup().wrapper,
                initialProps: { host: 'remote:a' },
            },
        );
        rerender({ host: 'remote:b' });
        await waitFor(() => expect(result.current.data).toBe('host B'));
        await act(async () => pending.resolve('host A'));
        expect(result.current.data).toBe('host B');
    });
});

it('同名容器切换引擎时不复用另一引擎的列表', async () => {
    const old = deferred<never[]>();
    service.listContainers.mockImplementation((_host, engine) =>
        engine === 'docker' ? old.promise : Promise.resolve([]),
    );
    const { wrapper } = setup();
    const { result, rerender } = renderHook(
        ({ engine }) => useDocker('local', 'containers', false, engine),
        {
            wrapper,
            initialProps: { engine: 'docker' as 'docker' | 'podman' },
        },
    );
    await waitFor(() => expect(service.listContainers).toHaveBeenCalledWith('local', 'docker'));
    rerender({ engine: 'podman' });
    await waitFor(() => expect(service.listContainers).toHaveBeenCalledWith('local', 'podman'));
    await act(async () => old.resolve([]));
    expect(result.current.containers).toEqual([]);
    await act(async () => {
        await result.current.containerActionAsync({ name: 'same-name', action: 'start' });
    });
    expect(service.containerAction).toHaveBeenCalledWith(
        'local',
        'same-name',
        'start',
        undefined,
        'podman',
    );
});
