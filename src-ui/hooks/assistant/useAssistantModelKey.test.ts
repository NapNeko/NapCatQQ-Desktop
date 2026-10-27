// @vitest-environment jsdom

import { createElement, type ReactNode } from 'react';
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantModelKey, useAssistantModelKeyDraft } from './useAssistantModelKey';
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
    it('缺少绑定origin时拒绝保存或清除，不发IPC', async () => {
        const { result } = mount();
        await act(async () => {
            expect(await result.current.save('unit-test-key', '')).toBe(false);
            expect(await result.current.clear('')).toBe(false);
        });
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.clearApiKey).not.toHaveBeenCalled();
    });
    it('只提交一次IPC，状态缓存和mutation变量均没有Key，不写浏览器存储', async () => {
        const storage = vi.spyOn(Storage.prototype, 'setItem');
        const { result, client } = mount();
        const key = 'unit-test-key-never-use-real';
        await act(async () => {
            expect(await result.current.save(key, 'https://models.example')).toBe(true);
        });
        expect(service.setApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).toHaveBeenCalledWith(key, 'https://models.example');
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
            expect(await result.current.save(key, 'https://models.example')).toBe(false);
        });
        expect(result.current.error).not.toContain(key);
        expect(result.current.error).toContain('[REDACTED]');
        expect(result.current.busy).toBe(false);
        expect(service.setApiKey).toHaveBeenCalledOnce();
        await act(async () => {
            expect(await result.current.save(key, 'https://models.example')).toBe(true);
        });
        expect(service.setApiKey).toHaveBeenCalledTimes(2);
    });
    it('清除只请求清除接口，并更新可用状态', async () => {
        const { result, client } = mount();
        client.setQueryData(assistantModelStatusKey, configured);
        await act(async () => {
            expect(await result.current.clear('https://models.example')).toBe(true);
        });
        expect(service.clearApiKey).toHaveBeenCalledOnce();
        expect(service.clearApiKey).toHaveBeenCalledWith('https://models.example');
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(client.getQueryData(assistantModelStatusKey)).toEqual({
            hasApiKey: false,
            configured: false,
            error: null,
        });
    });
    it('撤销清除失败状态后不再展示旧错误，也不重发请求', async () => {
        service.setApiKey.mockRejectedValueOnce(new Error('凭据存储暂不可用'));
        const { result } = mount();
        await act(async () => {
            expect(await result.current.save('unit-test-key', 'https://models.example')).toBe(
                false,
            );
        });
        expect(result.current.error).toBe('凭据存储暂不可用');
        act(() => result.current.reset());
        expect(result.current.error).toBeNull();
        expect(service.setApiKey).toHaveBeenCalledOnce();
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
            await result.current.save('unit-test-key', 'https://models.example');
        });
        await act(async () => {
            resolve({ hasApiKey: false, configured: false, error: null });
            await old;
        });
        expect(client.getQueryData(assistantModelStatusKey)).toEqual(configured);
    });
});

describe('未提交 Key 的 origin 绑定', () => {
    it('后台地址换源立即隐藏并清空Key，同源路径更新保留', () => {
        const { result, rerender } = renderHook(
            ({ baseUrl }) => useAssistantModelKeyDraft(baseUrl),
            {
                initialProps: { baseUrl: 'https://models.example/v1' },
            },
        );
        act(() => result.current.change('local-draft-unit-key'));
        rerender({ baseUrl: 'https://models.example/compat/v1' });
        expect(result.current.value).toBe('local-draft-unit-key');
        expect(result.current.origin).toBe('https://models.example');
        rerender({ baseUrl: 'https://other.example/v1' });
        expect(result.current.value).toBe('');
        expect(result.current.notice).toBe('服务地址已换源，未保存的 API Key 已清空。');
        rerender({ baseUrl: 'https://models.example/v1' });
        expect(result.current.value).toBe('');
    });
    it('异步保存失败只能恢复到原origin，当前地址变更后不会带回旧Key', () => {
        const { result, rerender } = renderHook(
            ({ baseUrl }) => useAssistantModelKeyDraft(baseUrl),
            {
                initialProps: { baseUrl: 'https://models.example/v1' },
            },
        );
        rerender({ baseUrl: 'https://other.example/v1' });
        act(() => result.current.change('failed-save-unit-key', 'https://models.example'));
        expect(result.current.value).toBe('');
        expect(result.current.notice).toBe('服务地址已换源，未保存的 API Key 已清空。');
    });
});
