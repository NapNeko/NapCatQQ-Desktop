import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import { NeoBotPanelError } from '../../core/domain/apps/neobotWorkspace';

const { call, push } = vi.hoisted(() => ({ call: vi.fn(), push: vi.fn() }));
vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: { panelCall: call },
}));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: push }));
import { useNeoBotAction } from './useNeoBotAction';

function mount() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(['neobotPanel', 'one', 'models'], {});
    client.setQueryData(['neobotPanel', 'one', 'env'], {});
    client.setQueryData(['neobotPanel', 'two', 'models'], {});
    const hook = renderHook(() => useNeoBotAction('one'), {
        wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
    });
    return { ...hook, client };
}

beforeEach(() => {
    call.mockReset();
    push.mockReset();
});

it('保留 PUT 乐观锁冲突的状态与服务器内容，不自动重试', async () => {
    const conflict = { ok: false, current: { version: 9, value: 'server' } };
    call.mockResolvedValue({ kind: 'failed', status: 409, message: '版本冲突', data: conflict });
    const { result } = mount();
    const body = { table: 'memory', key: 'group/1', version: 8, value: 'my draft' };
    await act(async () => {
        expect(
            await result.current.run({ method: 'PUT', path: '/api/archives/item', body }),
        ).toBeNull();
    });
    expect(call).toHaveBeenCalledTimes(1);
    expect(call).toHaveBeenCalledWith('one', 'PUT', '/api/archives/item', body);
    await waitFor(() => expect(result.current.error).toBeInstanceOf(NeoBotPanelError));
    expect(result.current.error).toMatchObject({ status: 409, data: conflict });
    expect(push).toHaveBeenCalledWith(
        expect.objectContaining({ tone: 'danger', content: '版本冲突' }),
    );
});

it('一次写入刷新当前实例的全部面板，保留其他实例缓存', async () => {
    call.mockResolvedValue({ kind: 'ok', data: { ok: true, applied: false } });
    const { result, client } = mount();
    await act(async () => {
        await result.current.run({
            path: '/api/config/env',
            body: { revision: 'hash-a', reload: true },
        });
    });
    expect(client.getQueryState(['neobotPanel', 'one', 'models'])?.isInvalidated).toBe(true);
    expect(client.getQueryState(['neobotPanel', 'one', 'env'])?.isInvalidated).toBe(true);
    expect(client.getQueryState(['neobotPanel', 'two', 'models'])?.isInvalidated).toBe(false);
    expect(push).toHaveBeenCalledWith(
        expect.objectContaining({
            tone: 'warning',
            content: expect.stringContaining('重载未成功'),
        }),
    );
});

it('仅保存没有请求重载时，applied:false 不误报失败', async () => {
    call.mockResolvedValue({
        kind: 'ok',
        data: { ok: true, applied: false, message: '配置已保存' },
    });
    const { result } = mount();
    await act(async () => {
        await result.current.run({
            path: '/api/config',
            body: { revision: 'hash-a', reload: false },
        });
    });
    expect(push).toHaveBeenCalledWith(
        expect.objectContaining({ tone: 'success', content: '配置已保存' }),
    );
});

it('探测失败是可展示的结果，普通写入的 ok:false 仍拒绝', async () => {
    call.mockResolvedValue({ kind: 'ok', data: { ok: false, message: '模型不存在' } });
    const { result } = mount();
    await act(async () => {
        expect(
            await result.current.run({
                path: '/api/config/models/test',
                quiet: true,
                allowNegative: true,
            }),
        ).toEqual({ ok: false, message: '模型不存在' });
    });
    expect(push).not.toHaveBeenCalled();
    await act(async () => {
        expect(await result.current.run({ path: '/api/config' })).toBeNull();
    });
    expect(push).toHaveBeenCalledWith(
        expect.objectContaining({ tone: 'danger', content: '模型不存在' }),
    );
});
