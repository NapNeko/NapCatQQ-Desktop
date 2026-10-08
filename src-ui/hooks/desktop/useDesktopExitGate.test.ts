import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { PrepareExitDesktopResponse } from '../../core/ipc/types';

type Listener = () => void;

const listeners: Record<'close' | 'blocked', Listener[]> = { close: [], blocked: [] };
const prepareExitDesktop = vi.fn<() => Promise<PrepareExitDesktopResponse>>();
const requestExitApp = vi.fn<() => Promise<void>>();
let holdListen: Promise<void> = Promise.resolve();

function subscribe(kind: 'close' | 'blocked') {
    return async (cb: Listener) => {
        await holdListen;
        listeners[kind].push(cb);
        return () => {
            listeners[kind] = listeners[kind].filter((l) => l !== cb);
        };
    };
}

vi.mock('../../core/domain/runtime/env', () => ({ isTauri: true }));
vi.mock('../../core/services/desktop.service', () => ({
    windowEventService: {
        onRequestClose: (cb: Listener) => subscribe('close')(cb),
        onExitBlocked: (cb: Listener) => subscribe('blocked')(cb),
    },
}));
vi.mock('../../core/services/exit.service', () => ({
    prepareExitDesktop: () => prepareExitDesktop(),
    requestExitApp: () => requestExitApp(),
}));

import { useDesktopExitGate } from './useDesktopExitGate';

const stats = (local: number): PrepareExitDesktopResponse => ({
    local_active: local,
    remote_active: 0,
    can_exit: local === 0,
});

beforeEach(() => {
    listeners.close = [];
    listeners.blocked = [];
    holdListen = Promise.resolve();
    prepareExitDesktop.mockReset();
    requestExitApp.mockReset();
});

describe('useDesktopExitGate', () => {
    it('收到关窗通知先问后端，本机没 Bot 在跑就弹确认', async () => {
        prepareExitDesktop.mockResolvedValue(stats(0));
        const { result } = renderHook(() => useDesktopExitGate());
        await waitFor(() => expect(listeners.close).toHaveLength(1));

        await act(async () => listeners.close[0]());
        expect(result.current).toMatchObject({ open: true, mode: 'confirm', stats: stats(0) });
    });

    it('托盘退出被拦下时弹无法退出', async () => {
        prepareExitDesktop.mockResolvedValue(stats(2));
        const { result } = renderHook(() => useDesktopExitGate());
        await waitFor(() => expect(listeners.blocked).toHaveLength(1));

        await act(async () => listeners.blocked[0]());
        expect(result.current).toMatchObject({ open: true, mode: 'blocked' });
    });

    it('监听还没挂好就卸载的，挂好后当场退掉', async () => {
        let release!: () => void;
        holdListen = new Promise<void>((r) => {
            release = r;
        });
        const { unmount } = renderHook(() => useDesktopExitGate());
        unmount();

        await act(async () => {
            release();
            await holdListen;
        });
        expect(listeners.close).toHaveLength(0);
        expect(listeners.blocked).toHaveLength(0);
    });

    it('退出请求失败时恢复按钮，并重新问一遍后端', async () => {
        prepareExitDesktop.mockResolvedValue(stats(0));
        requestExitApp.mockRejectedValueOnce('busy');
        const { result } = renderHook(() => useDesktopExitGate());
        await waitFor(() => expect(listeners.close).toHaveLength(1));
        await act(async () => listeners.close[0]());

        await act(async () => result.current.confirmExit());
        await waitFor(() => expect(result.current.exiting).toBe(false));
        expect(requestExitApp).toHaveBeenCalledTimes(1);
        expect(prepareExitDesktop).toHaveBeenCalledTimes(2);
    });
});
