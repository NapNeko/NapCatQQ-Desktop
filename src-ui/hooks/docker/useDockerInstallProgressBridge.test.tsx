// @vitest-environment jsdom
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DomainEventHandler } from '../events/useDomainEvents';
import { dockerActionStore } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import { useDockerInstallProgressBridge } from './useDockerInstallProgressBridge';

const events = vi.hoisted(() => ({ handler: undefined as DomainEventHandler | undefined }));
vi.mock('../events/useDomainEvents', () => ({
    useDomainEvents: (handler: DomainEventHandler) => {
        events.handler = handler;
    },
}));

const HOST_A = 'remote:a';
const HOST_B = 'remote:b';
let client: QueryClient;

beforeEach(() => {
    events.handler = undefined;
    client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } });
    dockerActionStore._reset();
    dockerInstallProgressStore._reset();
    for (const host of [HOST_A, HOST_B]) {
        client.setQueryData(['docker', 'status', host], { installed: false });
        client.setQueryData(['docker', 'containers', host], []);
    }
});

afterEach(() => {
    cleanup();
    client.clear();
    dockerActionStore.clearInstalling(HOST_A);
    dockerActionStore.clearInstalling(HOST_B);
    dockerActionStore._reset();
    dockerInstallProgressStore._reset();
});

function mountBridge() {
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    renderHook(() => useDockerInstallProgressBridge(), { wrapper });
}

function finish(taskId: string) {
    act(() => {
        events.handler?.({
            v: 1,
            kind: 'docker_install_progress',
            task_id: taskId,
            event: { v: 1, timestamp_ms: 0n, kind: 'finished', ok: true },
        });
    });
}

describe('Docker 安装进度主机隔离', () => {
    it('成功事件只刷新任务所属主机，并保留另一主机的安装锁', () => {
        dockerInstallProgressStore.started('install-a', HOST_A);
        dockerInstallProgressStore.started('install-b', HOST_B);
        dockerActionStore.markInstalling(HOST_A, 'install-a');
        dockerActionStore.markInstalling(HOST_B, 'install-b');
        mountBridge();
        finish('install-a');
        expect(dockerActionStore.getSnapshot().installingByHost[HOST_A]).toBeUndefined();
        expect(dockerActionStore.getSnapshot().installingByHost[HOST_B]).toBe(true);
        for (const kind of ['status', 'containers']) {
            expect(client.getQueryState(['docker', kind, HOST_A])?.isInvalidated).toBe(true);
            expect(client.getQueryState(['docker', kind, HOST_B])?.isInvalidated).toBe(false);
        }
    });

    it('未知任务的迟到事件不会刷新全部主机或解除别的安装锁', () => {
        dockerInstallProgressStore.started('install-b', HOST_B);
        dockerActionStore.markInstalling(HOST_B, 'install-b');
        mountBridge();
        finish('unknown-task');
        expect(dockerActionStore.getSnapshot().installingByHost[HOST_B]).toBe(true);
        expect(client.getQueryState(['docker', 'status', HOST_A])?.isInvalidated).toBe(false);
        expect(client.getQueryState(['docker', 'status', HOST_B])?.isInvalidated).toBe(false);
    });

    it('同主机旧任务迟到完成不会解除重试任务的安装锁', () => {
        dockerInstallProgressStore.started('old-install', HOST_A);
        dockerInstallProgressStore.started('retry-install', HOST_A);
        dockerActionStore.markInstalling(HOST_A, 'retry-install');
        mountBridge();
        finish('old-install');
        const snapshot = dockerActionStore.getSnapshot();
        expect(snapshot.installingByHost[HOST_A]).toBe(true);
        expect(snapshot.installTaskIdByHost[HOST_A]).toBe('retry-install');
    });
});
