import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppConfigData, ConfigFormSpec } from '../../core/domain/apps/appConfigForm';
import type {
    AppConfigWriteResult,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
} from '../../core/ipc/types';

const { readConfig, writeConfig } = vi.hoisted(() => ({
    readConfig: vi.fn(),
    writeConfig: vi.fn(),
}));
vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: { readConfig, writeConfig },
}));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: vi.fn() }));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { appConfigKey } from './useAppInstanceConfig';
import { useAppConfigForm } from './useAppConfigForm';

const spec: ConfigFormSpec<'nonebot2'> = {
    framework: 'nonebot2',
    validate: () => [],
    saveHint: () => null,
};
const clients: QueryClient[] = [];

function envelope(port: number, revision: string): AppInstanceConfigEnvelope {
    return {
        config: {
            framework: 'nonebot2',
            // 本测试只关心表单状态和版本，省略不会参与校验的其它框架字段。
            data: { env_prod: { port } } as AppConfigData<'nonebot2'>,
        },
        revision,
        documents: [],
    };
}

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((res) => {
        resolve = res;
    });
    return { promise, resolve };
}

function mount() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    clients.push(client);
    client.setQueryData(appConfigKey('one'), envelope(8080, 'a'));
    client.setQueryData(appConfigKey('two'), envelope(8181, 'other'));
    const hook = renderHook(
        ({ instanceId }) => useAppConfigForm(spec, instanceId, instanceId),
        {
            initialProps: { instanceId: 'one' },
            wrapper: ({ children }: { children: ReactNode }) => (
                <QueryClientProvider client={client}>{children}</QueryClientProvider>
            ),
        },
    );
    return { ...hook, client };
}

beforeEach(() => {
    readConfig.mockReset();
    writeConfig.mockReset();
    writeConfig.mockImplementation(
        async (_id: string, config: AppInstanceConfig): Promise<AppConfigWriteResult> => ({
            config,
            revision: 'saved',
            documents: [],
            restart_required: false,
            relinked: false,
            port_changed: false,
        }),
    );
});

afterEach(() => {
    cleanup();
    for (const client of clients.splice(0)) client.clear();
});

describe('useAppConfigForm', () => {
    it('外部刷新不盖掉草稿，撤销改动后灌入最新缓存并按新版本保存', async () => {
        const { result, client, rerender } = mount();
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const original = result.current.form!;
        act(() =>
            result.current.setForm({
                ...original,
                env_prod: { ...original.env_prod, port: 9000 },
            }),
        );
        act(() => {
            client.setQueryData(appConfigKey('one'), envelope(8081, 'b'));
        });
        rerender({ instanceId: 'one' });
        expect(result.current.form?.env_prod.port).toBe(9000);
        expect(result.current.dirty).toBe(true);

        act(() => result.current.reset());
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8081));
        expect(result.current.saved?.env_prod.port).toBe(8081);
        expect(result.current.dirty).toBe(false);

        const latest = result.current.form!;
        act(() =>
            result.current.setForm({
                ...latest,
                env_prod: { ...latest.env_prod, port: 8082 },
            }),
        );
        await act(async () => {
            await result.current.save();
        });
        expect(writeConfig).toHaveBeenCalledWith(
            'one',
            expect.objectContaining({
                data: expect.objectContaining({ env_prod: expect.objectContaining({ port: 8082 }) }),
            }),
            'b',
            undefined,
        );
    });

    it('重读相同缓存版本也会丢弃草稿并恢复保存基线', async () => {
        const { result, client, rerender } = mount();
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const original = result.current.form!;
        act(() =>
            result.current.setForm({
                ...original,
                env_prod: { ...original.env_prod, port: 9000 },
            }),
        );
        act(() => {
            client.setQueryData(appConfigKey('one'), envelope(8081, 'b'));
        });
        rerender({ instanceId: 'one' });
        const cached = client.getQueryData(appConfigKey('one'));
        readConfig.mockResolvedValue(envelope(8081, 'b'));

        await act(async () => {
            await result.current.reloadDiscard();
        });

        expect(client.getQueryData(appConfigKey('one'))).toBe(cached);
        expect(result.current.form?.env_prod.port).toBe(8081);
        expect(result.current.saved?.env_prod.port).toBe(8081);
        expect(result.current.dirty).toBe(false);
    });

    it('重读失败仍保留未保存草稿及原来的保存基线', async () => {
        const { result } = mount();
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const original = result.current.form!;
        act(() =>
            result.current.setForm({
                ...original,
                env_prod: { ...original.env_prod, port: 9000 },
            }),
        );
        readConfig.mockRejectedValue(new Error('读取失败'));

        await act(async () => {
            await result.current.reloadDiscard();
        });

        expect(result.current.form?.env_prod.port).toBe(9000);
        expect(result.current.saved?.env_prod.port).toBe(8080);
        expect(result.current.dirty).toBe(true);
    });

    it('切换实例后不把上一实例迟到的重读结果灌入当前表单', async () => {
        const { result, rerender } = mount();
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const pending = deferred<AppInstanceConfigEnvelope>();
        readConfig.mockReturnValue(pending.promise);
        let reload!: Promise<void>;
        act(() => {
            reload = result.current.reloadDiscard();
        });
        await waitFor(() => expect(readConfig).toHaveBeenCalledWith('one'));

        rerender({ instanceId: 'two' });
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8181));
        await act(async () => {
            pending.resolve(envelope(8081, 'b'));
            await reload;
        });

        expect(result.current.form?.env_prod.port).toBe(8181);
        expect(result.current.saved?.env_prod.port).toBe(8181);
        expect(result.current.dirty).toBe(false);
    });

    it('切走又切回同一实例后仍忽略上一轮重读，不覆盖新草稿', async () => {
        const { result, rerender } = mount();
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const pending = deferred<AppInstanceConfigEnvelope>();
        readConfig.mockReturnValue(pending.promise);
        let reload!: Promise<void>;
        act(() => {
            reload = result.current.reloadDiscard();
        });
        await waitFor(() => expect(readConfig).toHaveBeenCalledWith('one'));

        rerender({ instanceId: 'two' });
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8181));
        rerender({ instanceId: 'one' });
        await waitFor(() => expect(result.current.form?.env_prod.port).toBe(8080));
        const latest = result.current.form!;
        act(() =>
            result.current.setForm({
                ...latest,
                env_prod: { ...latest.env_prod, port: 9000 },
            }),
        );
        await act(async () => {
            pending.resolve(envelope(8081, 'b'));
            await reload;
        });

        expect(result.current.form?.env_prod.port).toBe(9000);
        expect(result.current.saved?.env_prod.port).toBe(8080);
        expect(result.current.dirty).toBe(true);
    });
});
