// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PropsWithChildren } from 'react';
import type { BatchResultResponse } from '../../core/ipc/types';
import { useRefreshBotStatuses } from './useRefreshBotStatuses';

const mocks = vi.hoisted(() => ({
    refresh: vi.fn(),
    napcat: vi.fn(),
    snowluma: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
}));
vi.mock('../../core/services/bot.service', () => ({
    botService: { refreshStatuses: mocks.refresh },
}));
vi.mock('../webui/napcatLoginStore', () => ({ napcatLoginStore: { refresh: mocks.napcat } }));
vi.mock('../webui/snowlumaStore', () => ({ snowlumaStore: { refresh: mocks.snowluma } }));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: mocks.success }));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: mocks.error }));
vi.mock('./useBotSnapshots', () => ({ botSnapshotsKey: ['botSnapshots'] }));

function fixture() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries').mockResolvedValue();
    const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { ...renderHook(useRefreshBotStatuses, { wrapper }), invalidate };
}

beforeEach(() => {
    mocks.refresh.mockReset();
    mocks.napcat.mockResolvedValue(undefined);
    mocks.snowluma.mockResolvedValue(undefined);
});
afterEach(cleanup);

describe('manual Bot status refresh', () => {
    it('runs a real probe once and stays busy through snapshot hydration', async () => {
        let finish!: (value: BatchResultResponse) => void;
        mocks.refresh.mockReturnValue(
            new Promise<BatchResultResponse>((resolve) => {
                finish = resolve;
            }),
        );
        let hydrate!: () => void;
        mocks.snowluma.mockReturnValue(
            new Promise<void>((resolve) => {
                hydrate = resolve;
            }),
        );
        const { result, invalidate } = fixture();
        let pending!: Promise<void>;
        act(() => {
            pending = result.current.refresh();
            void result.current.refresh();
        });
        expect(mocks.refresh).toHaveBeenCalledTimes(1);
        expect(result.current.refreshing).toBe(true);
        expect(invalidate).not.toHaveBeenCalled();
        await act(async () => {
            finish({ succeeded: ['10001'], failed: [] });
        });
        expect(invalidate).toHaveBeenCalledWith(
            { queryKey: ['botSnapshots'] },
            { throwOnError: true },
        );
        expect(mocks.napcat).toHaveBeenCalledOnce();
        expect(result.current.refreshing).toBe(true);
        await act(async () => {
            hydrate();
            await pending;
        });
        expect(result.current.refreshing).toBe(false);
        expect(mocks.success).toHaveBeenCalledOnce();
    });

    it('hydrates partial results and reports the failed Bot and reason', async () => {
        mocks.refresh.mockResolvedValue({
            succeeded: ['10001'],
            failed: [
                ['10002', 'SSH 超时'],
                ['10003', 'Docker 未就绪'],
            ],
        });
        const { result, invalidate } = fixture();
        await act(async () => {
            await result.current.refresh();
        });
        expect(invalidate).toHaveBeenCalledOnce();
        expect(mocks.error).toHaveBeenCalledWith(
            expect.objectContaining({
                raw: '10002：SSH 超时\n10003：Docker 未就绪',
                content: '10002：SSH 超时；10003：Docker 未就绪',
            }),
        );
        expect(mocks.success).not.toHaveBeenCalled();
        expect(result.current.refreshing).toBe(false);
    });

    it('recovers the button after command failure and permits another attempt', async () => {
        mocks.refresh
            .mockRejectedValueOnce(new Error('offline'))
            .mockResolvedValue({ succeeded: [], failed: [] });
        const { result, invalidate } = fixture();
        await act(async () => {
            await result.current.refresh();
        });
        expect(invalidate).toHaveBeenCalledOnce();
        expect(mocks.error).toHaveBeenCalledWith(expect.objectContaining({ raw: 'offline' }));
        await act(async () => {
            await result.current.refresh();
        });
        await waitFor(() => expect(result.current.refreshing).toBe(false));
        expect(mocks.refresh).toHaveBeenCalledTimes(2);
    });
});
