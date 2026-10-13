// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { PropsWithChildren } from 'react';
import type { DomainEvent } from '../../core/ipc/types';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { useChatTargets } from './useChatConversations';

const mocks = vi.hoisted(() => ({
    targets: vi.fn(),
    handlers: new Set<(event: DomainEvent) => void>(),
}));
vi.mock('../../core/services/chat.service', () => ({ chatService: { targets: mocks.targets } }));
vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (handler: (event: DomainEvent) => void) => {
        mocks.handlers.add(handler);
        return () => mocks.handlers.delete(handler);
    },
}));

const target: DebugTarget = {
    bot_id: '123456',
    name: 'Bot',
    qq_id: 123456,
    backend: 'snowluma',
    host: { kind: 'remote', server_id: 'test-server' },
    running: true,
    online: true,
};

beforeEach(() => mocks.targets.mockReset().mockResolvedValue([target]));
afterEach(cleanup);

function fixture() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return renderHook(useChatTargets, { wrapper });
}

it('updates offline and relogged accounts immediately on login events', async () => {
    const { result, unmount } = fixture();
    await waitFor(() => expect(result.current.data?.[0].online).toBe(true));
    for (const online of [false, true]) {
        mocks.targets.mockResolvedValue([{ ...target, online }]);
        act(() => {
            for (const handler of mocks.handlers)
                handler({
                    kind: 'snowluma_login_state_changed',
                    bot_id: target.bot_id,
                    state: online ? 'logged_in' : 'disconnected',
                });
        });
        await waitFor(() => expect(result.current.data?.[0].online).toBe(online));
    }
    expect(mocks.targets).toHaveBeenCalledTimes(3);
    unmount();
    expect(mocks.handlers.size).toBe(0);
});

it('refreshes unavailable probes without refetching for ordinary logs', async () => {
    const { result, unmount } = fixture();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    act(() => {
        for (const handler of mocks.handlers)
            handler({ kind: 'bot_log_appended', bot_id: target.bot_id, line: 'log' });
    });
    expect(mocks.targets).toHaveBeenCalledOnce();
    for (const kind of [
        'napcat_login_probe_unavailable',
        'snowluma_login_probe_unavailable',
    ] as const) {
        mocks.targets.mockResolvedValue([{ ...target, online: null }]);
        const count = mocks.targets.mock.calls.length;
        act(() => {
            for (const handler of mocks.handlers) handler({ kind, bot_id: target.bot_id });
        });
        await waitFor(() => expect(mocks.targets).toHaveBeenCalledTimes(count + 1));
        await waitFor(() => expect(result.current.isFetching).toBe(false));
    }
    expect(result.current.data?.[0].online).toBeNull();
    unmount();
});
