import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const subscribeMock = vi.fn();

vi.mock('../../core/services/event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

import { _resetDomainEventHubForTests } from '../../core/services/domain-event-hub';
import { useAppInstanceEventsBridge } from './useAppInstanceEventsBridge';
import { APP_INSTANCES_KEY } from './appInstancesCache';
import type { AppInstance, DeploymentTaskSnapshot, DomainEvent } from '../../core/ipc/types';

type StreamHandler = (event: DomainEvent) => void;
let emit: StreamHandler = () => {};

const installing: AppInstance = {
    id: '21e0e4a5',
    framework_id: 'maibot',
    display_name: '麦麦一号',
    placement: 'local_native',
    host_id: 'local',
    install_dir: '/apps/maibot/21e0e4a5',
    port: 29950,
    state: 'installing',
    created_at_ms: 1,
    install_renderer: true,
    origin: 'created',
};

function instanceChanged(instance: AppInstance): DomainEvent {
    return { kind: 'app_instance_changed', instance, reason: 'installed' } as DomainEvent;
}

function installTaskDone(status: 'success' | 'failed'): DomainEvent {
    const task: DeploymentTaskSnapshot = {
        taskId: 't1',
        kind: { kind: 'component_action', component_id: 'maibot', action: 'ensure_installed' },
        status,
        hostId: 'local',
        title: 'maibot@21e0e4a5 ensure_installed',
        resources: [{ kind: 'install_target', host_id: 'local', target: 'maibot@21e0e4a5' }],
        progressEvents: [],
        submittedAtMs: 1n,
        cancellable: false,
    };
    return { kind: 'deployment_task_changed', task } as DomainEvent;
}

async function flush(): Promise<void> {
    await Promise.resolve();
    await Promise.resolve();
}

function mountBridge(client: QueryClient) {
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return renderHook(() => useAppInstanceEventsBridge(), { wrapper });
}

beforeEach(() => {
    subscribeMock.mockReset();
    subscribeMock.mockImplementation(async (cb: StreamHandler) => {
        emit = cb;
        return () => {};
    });
    _resetDomainEventHubForTests();
});

afterEach(() => {
    vi.useRealTimers();
    _resetDomainEventHubForTests();
});

describe('useAppInstanceEventsBridge', () => {
    it('实例事件写进缓存，不管当时停在哪个页面', async () => {
        const client = new QueryClient();
        client.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, [installing]);
        mountBridge(client);
        await flush();

        emit(instanceChanged({ ...installing, state: 'installed', installed_version: '1.2.5' }));

        const list = client.getQueryData<AppInstance[]>(APP_INSTANCES_KEY);
        expect(list?.map((i) => i.state)).toEqual(['installed']);
    });

    it('列表还没拉过时不凭一条事件造出半张表', async () => {
        const client = new QueryClient();
        mountBridge(client);
        await flush();

        emit(instanceChanged({ ...installing, state: 'installed' }));

        expect(client.getQueryData(APP_INSTANCES_KEY)).toBeUndefined();
    });

    it('安装任务结束后迟迟没等到实例事件，就重拉列表兜底', async () => {
        vi.useFakeTimers();
        const client = new QueryClient();
        client.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, [installing]);
        const invalidate = vi.spyOn(client, 'invalidateQueries');
        mountBridge(client);
        await flush();

        emit(installTaskDone('success'));
        expect(invalidate).not.toHaveBeenCalled();

        vi.advanceTimersByTime(4_000);
        expect(invalidate).toHaveBeenCalledWith({ queryKey: APP_INSTANCES_KEY });
    });

    it('宽限期内已经收到「已安装」就不再重拉', async () => {
        vi.useFakeTimers();
        const client = new QueryClient();
        client.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, [installing]);
        const invalidate = vi.spyOn(client, 'invalidateQueries');
        mountBridge(client);
        await flush();

        emit(installTaskDone('success'));
        emit(instanceChanged({ ...installing, state: 'installed' }));
        vi.advanceTimersByTime(4_000);

        expect(invalidate).not.toHaveBeenCalled();
    });
});
