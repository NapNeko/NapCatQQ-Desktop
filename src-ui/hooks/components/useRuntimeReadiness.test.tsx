// @vitest-environment jsdom
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { RuntimeReadiness } from '../../core/ipc/types';
import { createDefaultBotConfig } from '../../core/domain/bot/config-defaults';
import { runtimeStartBlockReason } from '../../core/domain/bot/runtime-gate';
import {
    runtimeReadinessQueryKey,
    useRuntimeReadiness,
    useRuntimeReadinessMany,
} from './useRuntimeReadiness';

const service = vi.hoisted(() => ({ resolveRuntimeReadiness: vi.fn() }));
vi.mock('../../core/services/component.service', () => ({ componentService: service }));
vi.mock('../remote/useIsHostReachable', () => ({ useIsHostReachable: () => true }));

const TARGETS = [{ hostId: 'local', backend: 'napcat' as const, enabled: true }];
const KEY = runtimeReadinessQueryKey('local', 'napcat');
const CONFIG = createDefaultBotConfig();
const MISSING: RuntimeReadiness = {
    root: {
        target: { kind: 'component', id: 'napcat' },
        required_by: [],
        version_reqs: [],
        status: { state: 'missing' },
    },
    plan: { root: 'napcat', host_id: 'local', phase: 'run', nodes: [] },
};
const READY: RuntimeReadiness = {
    ...MISSING,
    root: { ...MISSING.root, status: { state: 'satisfied', version: null, source: null } },
};
const clients: QueryClient[] = [];

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function setup() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, gcTime: Infinity } },
    });
    client.setQueryData(KEY, MISSING);
    clients.push(client);
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { client, wrapper };
}

function useSingleReadiness() {
    return useRuntimeReadiness('local', 'napcat');
}

function useListReadiness() {
    return useRuntimeReadinessMany(TARGETS)['local|napcat']!;
}

beforeEach(() => vi.resetAllMocks());
afterEach(() => {
    cleanup();
    clients.splice(0).forEach((client) => client.clear());
});

describe.each([
    { label: '单个运行时', useState: useSingleReadiness },
    { label: 'Bot 列表运行时', useState: useListReadiness },
])('$label 重新探测', ({ useState }) => {
    it('安装后重查期间不再用旧缺失结果拦截，完成后立即放行', async () => {
        const pending = deferred<RuntimeReadiness>();
        service.resolveRuntimeReadiness.mockReturnValue(pending.promise);
        const { client, wrapper } = setup();
        const { result } = renderHook(useState, { wrapper });
        expect(runtimeStartBlockReason({ config: CONFIG, local: result.current })).toContain(
            '缺少',
        );

        act(() => {
            void client.invalidateQueries({ queryKey: KEY });
        });
        await waitFor(() => expect(result.current.probing).toBe(true));
        expect(result.current.readiness).toBeUndefined();
        expect(runtimeStartBlockReason({ config: CONFIG, local: result.current })).toContain(
            '正在确认',
        );

        await act(async () => pending.resolve(READY));
        await waitFor(() => expect(result.current.readiness).toEqual(READY));
        expect(result.current.probing).toBe(false);
        expect(runtimeStartBlockReason({ config: CONFIG, local: result.current })).toBeNull();
    });

    it('重查失败后不恢复旧缺失提示，重试时清掉旧错误', async () => {
        const first = deferred<RuntimeReadiness>();
        const retry = deferred<RuntimeReadiness>();
        service.resolveRuntimeReadiness
            .mockReturnValueOnce(first.promise)
            .mockReturnValueOnce(retry.promise);
        const { client, wrapper } = setup();
        const { result } = renderHook(useState, { wrapper });
        act(() => {
            void client.invalidateQueries({ queryKey: KEY });
        });
        await waitFor(() => expect(result.current.probing).toBe(true));
        await act(async () => first.reject(new Error('SSH probe failed')));
        await waitFor(() => expect(result.current.error).toContain('SSH probe failed'));
        expect(result.current.readiness).toBeUndefined();
        expect(result.current.probing).toBe(false);

        act(() => {
            void client.invalidateQueries({ queryKey: KEY });
        });
        await waitFor(() => expect(result.current.probing).toBe(true));
        expect(result.current.error).toBeNull();
        await act(async () => retry.resolve(READY));
        await waitFor(() => expect(result.current.readiness).toEqual(READY));
    });
});
