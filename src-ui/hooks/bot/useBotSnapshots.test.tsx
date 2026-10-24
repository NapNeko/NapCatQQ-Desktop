// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { PropsWithChildren } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotActorSnapshot, DomainEvent } from '../../core/ipc/types';
import { botSnapshotsKey, useBotSnapshots } from './useBotSnapshots';
import { useBotMutations } from './useBotMutations';
import { botLifecycleMutationKey } from './botLifecycleMutationKey';

const mocks = vi.hoisted(() => ({
    list: vi.fn(),
    stop: vi.fn(),
    start: vi.fn(),
    refreshStatuses: vi.fn(),
    reconcileRuntimes: vi.fn(),
    handler: undefined as ((event: DomainEvent) => void) | undefined,
}));
vi.mock('../../core/services/bot.service', () => ({
    botService: {
        listSnapshots: mocks.list,
        stop: mocks.stop,
        start: mocks.start,
        refreshStatuses: mocks.refreshStatuses,
        reconcileRuntimes: mocks.reconcileRuntimes,
    },
}));
vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (handler: (event: DomainEvent) => void) => {
        mocks.handler = handler;
        return () => {
            mocks.handler = undefined;
        };
    },
}));

const snapshot = (state: BotActorSnapshot['state'], revision: number): BotActorSnapshot => ({
    bot_id: 'one',
    state,
    revision,
    token_generation: 0,
    pending_restart: false,
});

function fixture() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { client, wrapper };
}

beforeEach(() => {
    vi.resetAllMocks();
});
afterEach(() => {
    cleanup();
    vi.useRealTimers();
});

describe('Bot card snapshot recovery', () => {
    it('does not let an in-flight list replace a newer terminal event', async () => {
        mocks.list.mockResolvedValueOnce([snapshot('running', 1)]);
        const { client, wrapper } = fixture();
        const hook = renderHook(() => useBotSnapshots({ disablePolling: true }), { wrapper });
        await waitFor(() => expect(hook.result.current.data?.[0].revision).toBe(1));
        let finish!: (snapshots: BotActorSnapshot[]) => void;
        mocks.list.mockImplementationOnce(
            () =>
                new Promise((resolve) => {
                    finish = resolve;
                }),
        );
        let pending!: Promise<unknown>;
        act(() => {
            pending = hook.result.current.refetch();
        });
        await waitFor(() => expect(finish).toBeTypeOf('function'));
        act(() => {
            mocks.handler?.({ kind: 'bot_state_changed', snapshot: snapshot('stopped', 3) });
        });
        await act(async () => {
            finish([snapshot('stopping', 2)]);
            await pending;
        });
        expect(client.getQueryData(botSnapshotsKey)).toEqual([snapshot('stopped', 3)]);
    });

    it('recovers a missing terminal event by reading local snapshots', async () => {
        vi.useFakeTimers();
        mocks.list
            .mockResolvedValueOnce([snapshot('stopping', 4)])
            .mockResolvedValue([snapshot('stopped', 5)]);
        const { wrapper } = fixture();
        const hook = renderHook(() => useBotSnapshots(), { wrapper });
        await act(async () => {
            await vi.advanceTimersByTimeAsync(5);
        });
        expect(hook.result.current.data?.[0].state).toBe('stopping');
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1100);
        });
        expect(hook.result.current.data?.[0].state).toBe('stopped');
        expect(mocks.list).toHaveBeenCalledTimes(2);
    });

    it('refetches the backend rollback state when stop returns an error', async () => {
        mocks.stop.mockRejectedValue(new Error('remote inspection failed'));
        const { client, wrapper } = fixture();
        const invalidated = vi.spyOn(client, 'invalidateQueries').mockResolvedValue();
        const message = vi.fn();
        const hook = renderHook(() => useBotMutations({ onMessage: message }), { wrapper });
        act(() => {
            hook.result.current.stopBot('one');
        });
        await waitFor(() =>
            expect(invalidated).toHaveBeenCalledWith({ queryKey: botSnapshotsKey }),
        );
        expect(message).toHaveBeenCalledWith(expect.objectContaining({ type: 'error' }));
    });

    it('polls only local snapshots every second while start is pending, including stopped preflight', async () => {
        vi.useFakeTimers();
        let observed = snapshot('stopped', 1);
        mocks.list.mockImplementation(async () => [observed]);
        let finish!: (value: BotActorSnapshot) => void;
        mocks.start.mockReturnValue(
            new Promise<BotActorSnapshot>((resolve) => {
                finish = resolve;
            }),
        );
        const { client, wrapper } = fixture();
        const hook = renderHook(
            () => ({ snapshots: useBotSnapshots(), mutations: useBotMutations() }),
            { wrapper },
        );
        await act(async () => {
            await vi.advanceTimersByTimeAsync(31_005);
        });
        const before = mocks.list.mock.calls.length;
        let pending!: Promise<BotActorSnapshot>;
        await act(async () => {
            pending = hook.result.current.mutations.startBotAsync('one');
            await vi.advanceTimersByTimeAsync(5);
        });
        expect(client.isMutating({ mutationKey: botLifecycleMutationKey })).toBe(1);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1100);
        });
        expect(mocks.list.mock.calls.length).toBeGreaterThan(before);
        expect(hook.result.current.snapshots.data?.[0].state).toBe('stopped');
        observed = snapshot('starting', 2);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1100);
        });
        expect(hook.result.current.snapshots.data?.[0].state).toBe('starting');
        expect(mocks.refreshStatuses).not.toHaveBeenCalled();
        expect(mocks.reconcileRuntimes).not.toHaveBeenCalled();
        observed = snapshot('running', 3);
        await act(async () => {
            finish(observed);
            await pending;
            await vi.advanceTimersByTimeAsync(5);
        });
        expect(client.isMutating({ mutationKey: botLifecycleMutationKey })).toBe(0);
        expect(hook.result.current.snapshots.data?.[0].state).toBe('running');
        const settled = mocks.list.mock.calls.length;
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1100);
        });
        expect(mocks.list).toHaveBeenCalledTimes(settled);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(14_000);
        });
        expect(mocks.list.mock.calls.length).toBeGreaterThan(settled);
    });

    it('settled consent rejection still reads the real local snapshot without an optimistic state', async () => {
        vi.useFakeTimers();
        let observed = snapshot('stopped', 1);
        mocks.list.mockImplementation(async () => [observed]);
        let fail!: (reason: Error) => void;
        mocks.start.mockReturnValue(
            new Promise<BotActorSnapshot>((_, reject) => {
                fail = reject;
            }),
        );
        const { client, wrapper } = fixture();
        const hook = renderHook(
            () => ({
                snapshots: useBotSnapshots({ disablePolling: true }),
                mutations: useBotMutations(),
            }),
            { wrapper },
        );
        await act(async () => {
            await vi.advanceTimersByTimeAsync(5);
        });
        let pending!: Promise<unknown>;
        await act(async () => {
            pending = hook.result.current.mutations.startBotAsync('one').catch((error) => error);
            await vi.advanceTimersByTimeAsync(5);
        });
        expect(hook.result.current.snapshots.data?.[0].state).toBe('stopped');
        const before = mocks.list.mock.calls.length;
        observed = snapshot('stopped', 3);
        await act(async () => {
            fail(new Error('SNOWLUMA_CONSENT_REQUIRED'));
            await pending;
            await vi.advanceTimersByTimeAsync(5);
        });
        expect(mocks.list.mock.calls.length).toBeGreaterThan(before);
        expect(client.getQueryData(botSnapshotsKey)).toEqual([observed]);
        expect(client.isMutating({ mutationKey: botLifecycleMutationKey })).toBe(0);
        expect(mocks.refreshStatuses).not.toHaveBeenCalled();
    });
});
