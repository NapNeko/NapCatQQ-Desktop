// @vitest-environment jsdom
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useDockerHosts } from './useDockerHosts';
import { dockerActionStore } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';

const service = vi.hoisted(() => ({
    probe: vi.fn(),
    imageReadyForFlavor: vi.fn(),
    listContainers: vi.fn(),
    install: vi.fn(),
    pullFrameworkImage: vi.fn(),
}));
vi.mock('../../core/services/docker.service', () => ({ dockerService: service }));
vi.mock('../useOpenExternal', () => ({ openExternalOrReport: vi.fn() }));

const HOST_A = 'remote:a';
const HOST_B = 'remote:b';
const HOSTS = [HOST_A, HOST_B];
const READY = {
    installed: true,
    daemonRunning: true,
    composeAvailable: true,
    version: '27.3.1',
};
const clients: QueryClient[] = [];

function setup() {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: Infinity },
            mutations: { retry: false, gcTime: Infinity },
        },
    });
    clients.push(client);
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { client, wrapper };
}

function requestsFor(hostId: string) {
    return {
        probes: service.probe.mock.calls.filter(([host]) => host === hostId).length,
        images: service.imageReadyForFlavor.mock.calls.filter(([host]) => host === hostId).length,
        containers: service.listContainers.mock.calls.filter(([host]) => host === hostId).length,
    };
}

beforeEach(() => {
    vi.resetAllMocks();
    dockerActionStore._reset();
    dockerInstallProgressStore._reset();
    service.probe.mockResolvedValue(READY);
    service.imageReadyForFlavor.mockResolvedValue(true);
    service.listContainers.mockResolvedValue([]);
    service.install.mockResolvedValue({
        status: 'installed',
        message: '已就绪',
        probedStatus: READY,
    });
    service.pullFrameworkImage.mockResolvedValue({});
});

afterEach(() => {
    cleanup();
    clients.splice(0).forEach((client) => client.clear());
    dockerActionStore.clearInstalling(HOST_A);
    dockerActionStore.clearInstalling(HOST_B);
    dockerActionStore._reset();
    dockerInstallProgressStore._reset();
    vi.restoreAllMocks();
});

describe('组件页 Docker 多主机查询', () => {
    it('探测错误透出后，不用旧的 daemon 缓存放行镜像和容器查询', async () => {
        const failure = new Error('Docker socket permission denied');
        service.probe.mockRejectedValue(failure);
        const { client, wrapper } = setup();
        const statusKey = ['docker', 'status', HOST_A];
        client.setQueryData(statusKey, READY);
        await client
            .fetchQuery({
                queryKey: statusKey,
                queryFn: () => service.probe(HOST_A),
                staleTime: 0,
                retry: false,
            })
            .catch(() => undefined);
        const { result } = renderHook(() => useDockerHosts([HOST_A]), { wrapper });
        await waitFor(() => expect(result.current.errorByHost[HOST_A]).toBe(failure));
        expect(result.current.statusByHost[HOST_A]?.daemonRunning).toBe(true);
        expect(service.imageReadyForFlavor).not.toHaveBeenCalled();
        expect(service.listContainers).not.toHaveBeenCalled();
    });

    it('镜像检查失败单独透出，不把错误显示为镜像未拉取', async () => {
        const failure = new Error('image inspect failed');
        service.imageReadyForFlavor.mockImplementation((_host: string, flavor: string) =>
            flavor === 'napcat' ? Promise.reject(failure) : Promise.resolve(true),
        );
        const { result } = renderHook(() => useDockerHosts([HOST_A]), {
            wrapper: setup().wrapper,
        });
        await waitFor(() => expect(result.current.imageErrorByHost[HOST_A]?.napcat).toBe(failure));
        expect(result.current.imageReadyByHost[HOST_A]?.napcat).toBeUndefined();
        await waitFor(() => expect(result.current.imageReadyByHost[HOST_A]?.snowluma).toBe(true));
        expect(result.current.errorByHost[HOST_A]).toBeNull();
    });

    it.each(['install', 'pull'] as const)(
        '%s 成功只刷新发起主机，不重新请求另一台主机',
        async (operation) => {
            const { client, wrapper } = setup();
            const { result } = renderHook(() => useDockerHosts(HOSTS), { wrapper });
            await waitFor(() => expect(service.listContainers).toHaveBeenCalledTimes(2));
            await waitFor(() => expect(client.isFetching()).toBe(0));
            const originalA = requestsFor(HOST_A);
            const originalB = requestsFor(HOST_B);
            await act(async () => {
                if (operation === 'install') await result.current.install(HOST_A);
                else await result.current.pullFrameworkImage(HOST_A, 'napcat', 'task-a');
            });
            await waitFor(() =>
                expect(requestsFor(HOST_A).probes).toBeGreaterThan(originalA.probes),
            );
            await waitFor(() => expect(client.isFetching()).toBe(0));
            expect(requestsFor(HOST_A).containers).toBeGreaterThan(originalA.containers);
            expect(requestsFor(HOST_B)).toEqual(originalB);
            expect(client.getQueryState(['docker', 'status', HOST_B])?.isInvalidated).toBe(false);
        },
    );

    it('安装成功即清理十分钟超时定时器', async () => {
        const { result } = renderHook(() => useDockerHosts([]), { wrapper: setup().wrapper });
        const schedule = vi.spyOn(globalThis, 'setTimeout');
        const cancel = vi.spyOn(globalThis, 'clearTimeout');
        await act(async () => {
            await result.current.install(HOST_A);
        });
        const timeoutIndex = schedule.mock.calls.findIndex(([, duration]) => duration === 600000);
        expect(timeoutIndex).toBeGreaterThanOrEqual(0);
        expect(cancel).toHaveBeenCalledWith(schedule.mock.results[timeoutIndex].value);
    });
});
