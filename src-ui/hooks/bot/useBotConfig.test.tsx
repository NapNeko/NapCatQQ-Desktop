import type { ReactNode } from 'react';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import { createDefaultBotConfig } from '../../core/domain/bot/config-defaults';

const service = vi.hoisted(() => ({
    getConfig: vi.fn(),
    upsertConfig: vi.fn(),
    upsertConfigWithDecisions: vi.fn(),
    deleteConfig: vi.fn(),
}));

vi.mock('../../core/services/bot.service', () => ({ botService: service }));
vi.mock('../events/useDomainEvents', () => ({ useDomainEvents: vi.fn() }));
vi.mock('./useBotSnapshots', () => ({ botSnapshotsKey: ['botSnapshots'] }));

import { useBotConfig } from './useBotConfig';

const clients: QueryClient[] = [];

function dockerConfig(): BotConfig {
    const config = createDefaultBotConfig();
    config.bot.QQID = 10001;
    config.bot.name = 'Docker Bot';
    config.bot.deploymentType = 'docker';
    config.bot.runtime_target = 'test-server';
    return config;
}

function mountConfig() {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: Infinity, refetchOnWindowFocus: false },
            mutations: { retry: false },
        },
    });
    clients.push(client);
    client.setQueryData(['botConfig', '10001'], dockerConfig());
    client.setQueryData(['botSnapshots'], []);
    client.setQueryData(['botFlavors'], {});
    const onSaved = vi.fn();
    const onError = vi.fn();
    const hook = renderHook(() => useBotConfig(null, { onSaved, onError }), {
        wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
    });
    return { ...hook, client, onSaved, onError };
}

beforeEach(() => {
    service.getConfig.mockReset().mockResolvedValue(null);
    service.upsertConfig.mockReset();
    service.upsertConfigWithDecisions.mockReset();
    service.deleteConfig.mockReset();
});

afterEach(() => {
    cleanup();
    for (const client of clients.splice(0)) client.clear();
});

describe('Docker 容器设置保存后的应用失败', () => {
    it.each(['save', 'saveWithDecisions'] as const)(
        '%s 将已落盘但重建失败视为部分成功，刷新配置与容器状态',
        async (method) => {
            const rawError = 'DOCKER_CONFIG_APPLY_FAILED: 容器启动失败';
            service.upsertConfig.mockRejectedValue(new Error(rawError));
            service.upsertConfigWithDecisions.mockRejectedValue(rawError);
            const { result, client, onSaved, onError } = mountConfig();
            const config = dockerConfig();

            act(() => {
                if (method === 'save') result.current.save(config);
                else result.current.saveWithDecisions(config, []);
            });

            await waitFor(() =>
                expect(onSaved).toHaveBeenCalledWith('10001', 'config_saved_docker_apply_failed'),
            );
            expect(onError).not.toHaveBeenCalled();
            expect(client.getQueryState(['botConfig', '10001'])?.isInvalidated).toBe(true);
            expect(client.getQueryState(['botSnapshots'])?.isInvalidated).toBe(true);
            expect(client.getQueryState(['botFlavors'])?.isInvalidated).toBe(true);
            await waitFor(() => expect(result.current.isSaving).toBe(false));
            if (method === 'save') expect(service.upsertConfig).toHaveBeenCalledWith(config);
            else expect(service.upsertConfigWithDecisions).toHaveBeenCalledWith(config, []);
        },
    );

    it.each(['save', 'saveWithDecisions'] as const)(
        '%s 的普通保存错误继续显示失败，不报告配置已保存',
        async (method) => {
            service.upsertConfig.mockRejectedValue(new Error('配置目录无法写入'));
            service.upsertConfigWithDecisions.mockRejectedValue('配置目录无法写入');
            const { result, client, onSaved, onError } = mountConfig();
            const config = dockerConfig();

            act(() => {
                if (method === 'save') result.current.save(config);
                else result.current.saveWithDecisions(config, []);
            });

            await waitFor(() =>
                expect(onError).toHaveBeenCalledWith(expect.stringContaining('配置目录无法写入')),
            );
            expect(onSaved).not.toHaveBeenCalled();
            expect(client.getQueryState(['botConfig', '10001'])?.isInvalidated).toBe(false);
            expect(client.getQueryState(['botSnapshots'])?.isInvalidated).toBe(false);
            await waitFor(() => expect(result.current.isSaving).toBe(false));
        },
    );
});
