// @vitest-environment jsdom

import { createElement, type ReactNode } from 'react';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotMigrationJob } from '../../core/ipc/generated/domain/BotMigrationJob';
import type { BotMigrationPlan } from '../../core/ipc/generated/domain/BotMigrationPlan';
import type { BotMigrationRequest } from '../../core/ipc/generated/domain/BotMigrationRequest';

const service = vi.hoisted(() => ({
    available: true,
    get: vi.fn(),
    preview: vi.fn(),
    start: vi.fn(),
    rollback: vi.fn(),
}));
vi.mock('../../core/services/bot-migration.service', () => ({ botMigrationService: service }));
vi.mock('./useBotSnapshots', () => ({ botSnapshotsKey: ['botSnapshots'] }));

import { isBotMigrationRunning, useBotMigration } from './useBotMigration';

function planFor(target: string): BotMigrationPlan {
    return {
        bot_id: '10001',
        source_host_id: 'remote:server-a',
        target_host_id: `remote:${target}`,
        apps: [],
        blockers: [],
        notes: [],
        fingerprint: `checked-${target}`,
    };
}

function jobFor(phase: BotMigrationJob['phase']): BotMigrationJob {
    return {
        id: 'migration-1',
        bot_id: '10001',
        source_host_id: 'remote:server-a',
        target_host_id: 'remote:server-b',
        phase,
        message: '迁移状态',
        apps: [],
        error: null,
        rollback_available: true,
        target_started: phase === 'awaiting_login',
        updated_at_ms: 1n,
    };
}

const clients: QueryClient[] = [];
function mountMigration(target = 'server-b') {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
            mutations: { retry: false },
        },
    });
    clients.push(client);
    const wrapper = ({ children }: { children: ReactNode }) =>
        createElement(QueryClientProvider, { client }, children);
    const rendered = renderHook(
        ({ targetServerId, open }) => useBotMigration('10001', targetServerId, open),
        { wrapper, initialProps: { targetServerId: target, open: true } },
    );
    return { ...rendered, client };
}

beforeEach(() => {
    service.available = true;
    service.get.mockReset().mockResolvedValue(null);
    service.preview
        .mockReset()
        .mockImplementation(async (request: BotMigrationRequest) =>
            planFor(request.target_server_id),
        );
    service.start.mockReset().mockResolvedValue(jobFor('preparing'));
    service.rollback.mockReset().mockResolvedValue(jobFor('rolling_back'));
});

afterEach(() => {
    cleanup();
    for (const client of clients) client.clear();
    clients.length = 0;
});

describe('Bot 迁移阶段', () => {
    it.each([
        'preparing',
        'stopping',
        'copying',
        'rebuilding',
        'switching',
        'starting',
        'awaiting_login',
        'rolling_back',
    ] as const)('%s 期间禁止重复发起迁移并继续跟踪', (phase) => {
        expect(isBotMigrationRunning(phase)).toBe(true);
    });

    it.each(['completed', 'failed', 'rolled_back', 'recovery_required'] as const)(
        '%s 保留报告且停止自动轮询',
        (phase) => expect(isBotMigrationRunning(phase)).toBe(false),
    );
});

describe('useBotMigration', () => {
    it('切换目标后，迟到的旧预检不能启动新目标', async () => {
        let resolveOld!: (plan: BotMigrationPlan) => void;
        let resolveNew!: (plan: BotMigrationPlan) => void;
        const oldPreview = new Promise<BotMigrationPlan>((resolve) => {
            resolveOld = resolve;
        });
        const newPreview = new Promise<BotMigrationPlan>((resolve) => {
            resolveNew = resolve;
        });
        service.preview.mockImplementation((request: BotMigrationRequest) =>
            request.target_server_id === 'server-b' ? oldPreview : newPreview,
        );
        const { result, rerender } = mountMigration('server-b');
        await waitFor(() =>
            expect(service.preview).toHaveBeenCalledWith({
                bot_id: '10001',
                target_server_id: 'server-b',
            }),
        );

        rerender({ targetServerId: 'server-c', open: true });
        await waitFor(() =>
            expect(service.preview).toHaveBeenCalledWith({
                bot_id: '10001',
                target_server_id: 'server-c',
            }),
        );
        await act(async () => {
            resolveOld(planFor('server-b'));
        });

        expect(result.current.previewQuery.data).toBeUndefined();
        await act(async () => {
            await expect(result.current.startMutation.mutateAsync()).rejects.toThrow(
                '预检尚未通过',
            );
        });
        expect(service.start).not.toHaveBeenCalled();

        await act(async () => {
            resolveNew(planFor('server-c'));
        });
        await waitFor(() => expect(result.current.previewQuery.isSuccess).toBe(true));
        await act(async () => {
            await result.current.startMutation.mutateAsync();
        });
        expect(service.start).toHaveBeenCalledTimes(1);
        expect(service.start).toHaveBeenCalledWith(
            { bot_id: '10001', target_server_id: 'server-c' },
            'checked-server-c',
        );
    });

    it('预检阻断时无法开始，错误保留给弹窗', async () => {
        service.preview.mockResolvedValue({
            ...planFor('server-b'),
            blockers: ['目标端目录已存在'],
        });
        const { result } = mountMigration();
        await waitFor(() => expect(result.current.previewQuery.isSuccess).toBe(true));

        await act(async () => {
            await expect(result.current.startMutation.mutateAsync()).rejects.toThrow(
                '预检尚未通过',
            );
        });
        await waitFor(() =>
            expect(result.current.startMutation.error?.message).toContain('预检尚未通过'),
        );
        expect(service.start).not.toHaveBeenCalled();
    });

    it('执行失败后保留可读错误并重新读取后台记录', async () => {
        const error = new Error('连接目标服务器失败');
        service.start.mockRejectedValue(error);
        const { result } = mountMigration();
        await waitFor(() => expect(result.current.previewQuery.isSuccess).toBe(true));
        const readsBeforeStart = service.get.mock.calls.length;

        await act(async () => {
            await expect(result.current.startMutation.mutateAsync()).rejects.toBe(error);
        });
        await waitFor(() => expect(result.current.startMutation.error).toBe(error));
        expect(service.get.mock.calls.length).toBeGreaterThan(readsBeforeStart);
    });

    it('读取记录或预检失败都能交给弹窗显示', async () => {
        const readError = new Error('迁移记录暂时无法读取');
        service.get.mockRejectedValue(readError);
        const first = mountMigration();
        await waitFor(() => expect(first.result.current.jobQuery.error).toBe(readError));
        expect(service.preview).not.toHaveBeenCalled();
        first.unmount();

        service.get.mockResolvedValue(null);
        const previewError = new Error('源服务器离线');
        service.preview.mockRejectedValue(previewError);
        const second = mountMigration();
        await waitFor(() => expect(second.result.current.previewQuery.error).toBe(previewError));
    });

    it('等待扫码时不能重复开始，但允许确认数据风险后回退', async () => {
        service.get.mockResolvedValue(jobFor('awaiting_login'));
        const { result, rerender } = mountMigration();
        await waitFor(() => expect(result.current.job?.phase).toBe('awaiting_login'));
        expect(result.current.running).toBe(true);
        expect(service.preview).not.toHaveBeenCalled();

        rerender({ targetServerId: 'server-b', open: false });
        await act(async () => {
            await result.current.rollbackMutation.mutateAsync(true);
        });
        expect(service.rollback).toHaveBeenCalledTimes(1);
        expect(service.rollback).toHaveBeenCalledWith('migration-1', true);
        await waitFor(() => expect(result.current.job?.phase).toBe('rolling_back'));
    });

    it('浏览器预览不会请求迁移接口', async () => {
        service.available = false;
        const { result } = mountMigration();
        expect(result.current.available).toBe(false);
        expect(service.get).not.toHaveBeenCalled();
        expect(service.preview).not.toHaveBeenCalled();
        expect(service.start).not.toHaveBeenCalled();
    });

    it('回退请求失败后保留错误和原任务报告', async () => {
        const error = new Error('目标端停止失败，尚未切回源服务器');
        service.get.mockResolvedValue(jobFor('awaiting_login'));
        service.rollback.mockRejectedValue(error);
        const { result } = mountMigration();
        await waitFor(() => expect(result.current.job?.phase).toBe('awaiting_login'));

        await act(async () => {
            await expect(result.current.rollbackMutation.mutateAsync(true)).rejects.toBe(error);
        });
        await waitFor(() => expect(result.current.rollbackMutation.error).toBe(error));
        expect(result.current.job?.id).toBe('migration-1');
        expect(result.current.job?.phase).toBe('awaiting_login');
    });

    it('终态报告不自动对已迁移的目标重新预检', async () => {
        service.get.mockResolvedValue(jobFor('completed'));
        const { result } = mountMigration();
        await waitFor(() => expect(result.current.job?.phase).toBe('completed'));
        expect(service.preview).not.toHaveBeenCalled();
    });

    it('迟到的旧记录不能覆盖刚开始的回退并停止跟踪', async () => {
        service.get.mockResolvedValueOnce(jobFor('completed'));
        const { result } = mountMigration();
        await waitFor(() => expect(result.current.job?.phase).toBe('completed'));

        let finishOldRead!: (job: BotMigrationJob) => void;
        service.get.mockImplementation(
            () => new Promise<BotMigrationJob>((resolve) => (finishOldRead = resolve)),
        );
        let oldRead!: ReturnType<typeof result.current.jobQuery.refetch>;
        act(() => {
            oldRead = result.current.jobQuery.refetch();
        });
        await waitFor(() => expect(finishOldRead).toBeTypeOf('function'));
        await act(async () => {
            await result.current.rollbackMutation.mutateAsync(true);
        });
        await waitFor(() => expect(result.current.job?.phase).toBe('rolling_back'));
        await act(async () => {
            finishOldRead(jobFor('completed'));
            await oldRead;
        });
        expect(result.current.job?.phase).toBe('rolling_back');
        expect(result.current.running).toBe(true);
    });
});
