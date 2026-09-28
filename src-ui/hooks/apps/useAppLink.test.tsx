import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { OneBotLinkPlan } from '../../core/ipc/types';

const previewLink = vi.fn();

vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: {
        previewLink: (...args: unknown[]) => previewLink(...args),
    },
}));

import { useAppLinkPlan } from './useAppLink';

function planFor(botId: string): OneBotLinkPlan {
    return { bot_id: botId } as OneBotLinkPlan;
}

function deferred<T>() {
    let resolve!: (v: T) => void;
    const promise = new Promise<T>((r) => {
        resolve = r;
    });
    return { promise, resolve };
}

function mount(initial: { instanceId: string; botId: string; enabled: boolean }) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return renderHook((p: typeof initial) => useAppLinkPlan(p.instanceId, p.botId, p.enabled), {
        wrapper,
        initialProps: initial,
    });
}

beforeEach(() => {
    previewLink.mockReset();
});

describe('useAppLinkPlan', () => {
    it('没打开或没选齐时不去要计划', () => {
        const { result, rerender } = mount({ instanceId: 'i1', botId: '', enabled: true });
        rerender({ instanceId: 'i1', botId: '10001', enabled: false });
        expect(result.current).toEqual({ plan: null, previewing: false });
        expect(previewLink).not.toHaveBeenCalled();
    });

    it('换了 Bot 重新生成的途中不再给上一份计划', async () => {
        previewLink.mockResolvedValueOnce(planFor('10001'));
        const { result, rerender } = mount({ instanceId: 'i1', botId: '10001', enabled: true });
        await waitFor(() => expect(result.current.plan?.bot_id).toBe('10001'));

        const next = deferred<OneBotLinkPlan>();
        previewLink.mockReturnValueOnce(next.promise);
        rerender({ instanceId: 'i1', botId: '10002', enabled: true });
        expect(result.current).toEqual({ plan: null, previewing: true });

        next.resolve(planFor('10002'));
        await waitFor(() => expect(result.current.plan?.bot_id).toBe('10002'));
        expect(result.current.previewing).toBe(false);
    });

    it('生成失败时没有计划可点，也不自己重试', async () => {
        previewLink.mockRejectedValueOnce('Bot 不在线');
        const { result } = mount({ instanceId: 'i1', botId: '10001', enabled: true });
        await waitFor(() => expect(result.current.previewing).toBe(false));
        expect(result.current.plan).toBeNull();
        expect(previewLink).toHaveBeenCalledTimes(1);
    });
});
