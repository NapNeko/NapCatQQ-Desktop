import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { BotRuntimeMetrics } from '../../core/ipc/generated/domain/BotRuntimeMetrics';
import type { BotRuntimeMetricsCatalogState } from './botRuntimeMetricsCatalog';

const mocked = vi.hoisted(() => ({
    getMetrics: vi.fn(),
    listeners: new Set<() => void>(),
    catalog: {} as BotRuntimeMetricsCatalogState,
}));

vi.mock('../../core/services/bot.service', () => ({
    botService: { getRuntimeMetrics: mocked.getMetrics },
}));
vi.mock('./botRuntimeMetricsCatalog', () => ({
    getBotRuntimeMetricsCatalogSnapshot: () => mocked.catalog,
    subscribeBotRuntimeMetricsCatalog: (listener: () => void) => {
        mocked.listeners.add(listener);
        return () => mocked.listeners.delete(listener);
    },
}));

import { useBotRuntimeMetrics } from './useBotRuntimeMetrics';

function metrics(botId: string, collectedAt = 1000): BotRuntimeMetrics {
    return {
        v: 1,
        bot_id: botId,
        collected_at_ms: collectedAt,
        source: 'probe',
        probe: 'active',
        nodes: [],
    };
}

function deferred() {
    let resolve!: (value: BotRuntimeMetrics) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<BotRuntimeMetrics>((ok, fail) => {
        resolve = ok;
        reject = fail;
    });
    return { promise, resolve, reject };
}

let client: QueryClient;
const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
);

async function settle() {
    await act(async () => {
        await vi.advanceTimersByTimeAsync(1);
    });
}

function updateCatalog(patch: Partial<BotRuntimeMetricsCatalogState>) {
    act(() => {
        mocked.catalog = { ...mocked.catalog, ...patch };
        for (const listener of mocked.listeners) listener();
    });
}

beforeEach(() => {
    vi.useFakeTimers();
    mocked.getMetrics.mockReset();
    mocked.catalog = {
        enabled: true,
        intervalMs: 1000,
        retentionDays: 7,
        byId: {},
        lastFetchAt: 1,
        fetching: false,
    };
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
});

afterEach(() => {
    cleanup();
    client.clear();
    mocked.listeners.clear();
    vi.useRealTimers();
});

describe('useBotRuntimeMetrics', () => {
    it('慢采集期间轮询与手动刷新共用在途请求，失败后下一轮仍能采集', async () => {
        const first = deferred();
        mocked.getMetrics.mockReturnValueOnce(first.promise).mockResolvedValue(metrics('1', 2000));
        const { result } = renderHook(() => useBotRuntimeMetrics('1', { liveDetail: true }), {
            wrapper,
        });
        await settle();
        let refresh!: Promise<void>;
        act(() => {
            refresh = result.current.refresh();
        });
        await act(async () => {
            await vi.advanceTimersByTimeAsync(3000);
        });
        expect(mocked.getMetrics).toHaveBeenCalledTimes(1);
        await act(async () => {
            first.reject(new Error('SSH 暂时不可用'));
            await refresh;
        });
        await settle();
        expect(result.current.loading).toBe(false);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        await settle();
        expect(mocked.getMetrics).toHaveBeenCalledTimes(2);
        expect(result.current.metrics?.collected_at_ms).toBe(2000);
    });

    it('已有采样后的手动刷新也不会取消轮询并另发一份 IPC', async () => {
        const next = deferred();
        mocked.getMetrics.mockResolvedValueOnce(metrics('1')).mockReturnValueOnce(next.promise);
        const { result } = renderHook(() => useBotRuntimeMetrics('1', { liveDetail: true }), {
            wrapper,
        });
        await settle();
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1000);
        });
        let refresh!: Promise<void>;
        act(() => {
            refresh = result.current.refresh();
        });
        await settle();
        expect(mocked.getMetrics).toHaveBeenCalledTimes(2);
        expect(result.current.loading).toBe(false);
        await act(async () => {
            next.resolve(metrics('1', 2000));
            await refresh;
        });
        await settle();
        expect(result.current.metrics?.collected_at_ms).toBe(2000);
    });

    it('切换 Bot 立即不显示旧数据，旧 Bot 手动刷新迟到也不能覆盖新 Bot', async () => {
        const oldRefresh = deferred();
        const newBot = deferred();
        mocked.getMetrics
            .mockResolvedValueOnce(metrics('1'))
            .mockReturnValueOnce(oldRefresh.promise)
            .mockReturnValueOnce(newBot.promise);
        const { result, rerender } = renderHook(
            ({ botId }) => useBotRuntimeMetrics(botId, { liveDetail: true }),
            { initialProps: { botId: '1' }, wrapper },
        );
        await settle();
        expect(result.current.metrics?.bot_id).toBe('1');
        let refresh!: Promise<void>;
        act(() => {
            refresh = result.current.refresh();
        });
        rerender({ botId: '2' });
        expect(result.current.metrics).toBeNull();
        await act(async () => newBot.resolve(metrics('2', 2000)));
        await settle();
        expect(result.current.metrics?.bot_id).toBe('2');
        await act(async () => {
            oldRefresh.resolve(metrics('1', 3000));
            await refresh;
        });
        await settle();
        expect(result.current.metrics?.bot_id).toBe('2');
    });

    it('列表和单拉并行完成时选择较新采样', async () => {
        const detail = deferred();
        mocked.getMetrics.mockReturnValue(detail.promise);
        const { result } = renderHook(() => useBotRuntimeMetrics('1', { liveDetail: true }), {
            wrapper,
        });
        updateCatalog({ byId: { '1': metrics('1', 2000) } });
        await act(async () => detail.resolve(metrics('1', 1000)));
        await settle();
        expect(result.current.metrics?.collected_at_ms).toBe(2000);
    });

    it('补拉数据也按 Bot 隔离，关闭监控后不显示已有采样', async () => {
        const next = deferred();
        mocked.getMetrics.mockResolvedValueOnce(metrics('1')).mockReturnValueOnce(next.promise);
        const { result, rerender } = renderHook(({ botId }) => useBotRuntimeMetrics(botId), {
            initialProps: { botId: '1' },
            wrapper,
        });
        await settle();
        expect(result.current.metrics?.bot_id).toBe('1');
        rerender({ botId: '2' });
        expect(result.current.metrics).toBeNull();
        await act(async () => next.resolve(metrics('2')));
        await settle();
        expect(result.current.metrics?.bot_id).toBe('2');
        updateCatalog({ enabled: false });
        expect(result.current.metrics).toBeNull();
        expect(result.current.loading).toBe(false);
    });

    it('关闭再开启监控后，关闭前的在途响应不能覆盖新采样', async () => {
        const oldRequest = deferred();
        const newRequest = deferred();
        mocked.getMetrics
            .mockReturnValueOnce(oldRequest.promise)
            .mockReturnValueOnce(newRequest.promise);
        const { result } = renderHook(() => useBotRuntimeMetrics('1', { liveDetail: true }), {
            wrapper,
        });
        await settle();
        updateCatalog({ enabled: false });
        await settle();
        updateCatalog({ enabled: true });
        await settle();
        expect(mocked.getMetrics).toHaveBeenCalledTimes(2);
        await act(async () => newRequest.resolve(metrics('1', 2000)));
        await settle();
        await act(async () => oldRequest.resolve(metrics('1', 1000)));
        await settle();
        expect(result.current.metrics?.collected_at_ms).toBe(2000);
    });
});
