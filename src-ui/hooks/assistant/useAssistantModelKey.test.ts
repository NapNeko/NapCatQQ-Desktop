// @vitest-environment jsdom

import { createElement, type ReactNode } from 'react';
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantModelKey } from './useAssistantModelKey';
import { assistantModelStatusKey } from './useAssistant';

const service = vi.hoisted(() => ({ setApiKey: vi.fn(), clearApiKey: vi.fn() }));
vi.mock('../../core/services/assistant.service', () => ({ assistantService: service }));
const clients: QueryClient[] = [];
const configured = { hasApiKey: true, configured: true, error: null };
function mount() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    clients.push(client);
    const wrapper = ({ children }: { children: ReactNode }) =>
        createElement(QueryClientProvider, { client }, children);
    return { ...renderHook(() => useAssistantModelKey(), { wrapper }), client };
}
beforeEach(() => {
    service.setApiKey.mockReset().mockResolvedValue(configured);
    service.clearApiKey
        .mockReset()
        .mockResolvedValue({ hasApiKey: false, configured: false, error: null });
});
afterEach(() => {
    cleanup();
    clients.forEach((client) => client.clear());
    clients.length = 0;
    vi.restoreAllMocks();
});

describe('API Key 提交边界', () => {
    it('只提交一次IPC，状态缓存和mutation变量均没有Key，不写浏览器存储', async () => {
        const storage = vi.spyOn(Storage.prototype, 'setItem');
        const { result, client } = mount();
        const key = 'unit-test-key-never-use-real';
        await act(async () => {
            expect(await result.current.save(key)).toBe(true);
        });
        expect(service.setApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).toHaveBeenCalledWith(key);
        expect(client.getQueryData(assistantModelStatusKey)).toEqual(configured);
        expect(client.getMutationCache().getAll()).toEqual([]);
        expect(
            JSON.stringify(
                client
                    .getQueryCache()
                    .getAll()
                    .map((query) => query.state.data),
            ),
        ).not.toContain(key);
        expect(storage).not.toHaveBeenCalled();
    });
    it('服务端异常即使回显提交内容，也不会出现在错误状态或自动重试', async () => {
        const key = 'unit-test-rejected-key';
        service.setApiKey.mockRejectedValueOnce(new Error(`拒绝：${key}`));
        const { result } = mount();
        await act(async () => {
            expect(await result.current.save(key)).toBe(false);
        });
        expect(result.current.error).not.toContain(key);
        expect(result.current.error).toContain('[REDACTED]');
        expect(result.current.busy).toBe(false);
        expect(service.setApiKey).toHaveBeenCalledOnce();
        await act(async () => {
            expect(await result.current.save(key)).toBe(true);
        });
        expect(service.setApiKey).toHaveBeenCalledTimes(2);
    });
    it('清除只请求清除接口，并更新可用状态', async () => {
        const { result, client } = mount();
        client.setQueryData(assistantModelStatusKey, configured);
        await act(async () => {
            expect(await result.current.clear()).toBe(true);
        });
        expect(service.clearApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(client.getQueryData(assistantModelStatusKey)).toEqual({
            hasApiKey: false,
            configured: false,
            error: null,
        });
    });
    it('较早的状态查询不能在Key保存后回滚为未配置', async () => {
        const { result, client } = mount();
        let resolve!: (value: typeof configured) => void;
        const old = client
            .fetchQuery({
                queryKey: assistantModelStatusKey,
                queryFn: () =>
                    new Promise<typeof configured>((done) => {
                        resolve = done;
                    }),
            })
            .catch(() => undefined);
        await act(async () => {
            await result.current.save('unit-test-key');
        });
        await act(async () => {
            resolve({ hasApiKey: false, configured: false, error: null });
            await old;
        });
        expect(client.getQueryData(assistantModelStatusKey)).toEqual(configured);
    });
});
