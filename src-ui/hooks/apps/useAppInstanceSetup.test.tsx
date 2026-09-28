import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { AppProjectProbe } from '../../core/ipc/types';

const probeProject = vi.fn();

vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: {
        probeProject: (...args: unknown[]) => probeProject(...args),
    },
}));

import { useProbeAppProject } from './useAppInstanceSetup';

function deferred<T>() {
    let resolve!: (v: T) => void;
    let reject!: (e: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function mount() {
    const client = new QueryClient();
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return renderHook(() => useProbeAppProject(), { wrapper });
}

// react-query 的通知排在下一个宏任务里，断言「没发生」之前先让它走完
const tick = () => new Promise((r) => setTimeout(r, 10));

const vars = { hostId: 'local', frameworkId: 'karin', path: 'D:/old' };
const probeOf = (path: string) => ({ path, display_name: 'old' }) as AppProjectProbe;

beforeEach(() => {
    probeProject.mockReset();
});

// 导入对话框靠 reset 作废路上那次检查，这里钉住它依赖的行为
describe('useProbeAppProject', () => {
    it('没 reset 时结果进 data，单次回调照常触发', async () => {
        const pending = deferred<AppProjectProbe>();
        probeProject.mockReturnValueOnce(pending.promise);
        const onSuccess = vi.fn();
        const { result } = mount();

        act(() => result.current.mutate(vars, { onSuccess }));
        await act(async () => {
            pending.resolve(probeOf('D:/old'));
            await pending.promise;
            await tick();
        });
        expect(result.current.data?.path).toBe('D:/old');
        expect(onSuccess).toHaveBeenCalledTimes(1);
    });

    it('reset 之后晚到的结果不进 data，单次回调也不触发', async () => {
        const pending = deferred<AppProjectProbe>();
        probeProject.mockReturnValueOnce(pending.promise);
        const onSuccess = vi.fn();
        const { result } = mount();

        act(() => result.current.mutate(vars, { onSuccess }));
        await waitFor(() => expect(result.current.isPending).toBe(true));
        act(() => result.current.reset());
        await waitFor(() => expect(result.current.isPending).toBe(false));

        await act(async () => {
            pending.resolve(probeOf('D:/old'));
            await pending.promise;
            await tick();
        });
        expect(result.current.data).toBeUndefined();
        expect(onSuccess).not.toHaveBeenCalled();
    });

    it('reset 之后晚到的失败也不触发单次回调', async () => {
        const pending = deferred<AppProjectProbe>();
        probeProject.mockReturnValueOnce(pending.promise);
        const onError = vi.fn();
        const { result } = mount();

        act(() => result.current.mutate(vars, { onError }));
        act(() => result.current.reset());

        await act(async () => {
            pending.reject('目录不存在');
            await pending.promise.catch(() => undefined);
            await tick();
        });
        expect(result.current.isError).toBe(false);
        expect(onError).not.toHaveBeenCalled();
    });
});
