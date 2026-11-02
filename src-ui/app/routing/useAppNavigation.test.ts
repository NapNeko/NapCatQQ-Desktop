import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppNavigation } from './useAppNavigation';

const state = vi.hoisted(() => ({
    features: { apps: true, chat: true, dockerPage: true },
    debugEnabled: true,
    focusChatIfOpen: vi.fn(),
    focusDebugIfOpen: vi.fn(),
    preload: vi.fn(),
    error: vi.fn(),
    window: null as null | { showChat: () => void; releaseChat: () => (() => void) | undefined },
}));
vi.mock('../../hooks/preferences/featureTogglesStore', () => ({
    useFeatures: () => state.features,
    featureTogglesStore: { getSnapshot: () => state.features },
}));
vi.mock('../../hooks/debug/useDebugConsoleEnabled', () => ({
    useDebugConsoleEnabled: () => state.debugEnabled,
}));
vi.mock('../../core/domain/runtime/env', () => ({ localOs: () => 'linux' }));
vi.mock('../../hooks/remote/useServers', () => ({ useServers: () => ({ data: undefined }) }));
vi.mock('../../hooks/desktop/useAppWindowBridge', () => ({
    useAppWindowBridge: (callbacks: NonNullable<typeof state.window>) => {
        state.window = callbacks;
        return state;
    },
}));
vi.mock('../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: state.error }));
vi.mock('./pageLoaders', () => ({ preloadRoute: state.preload }));

beforeEach(() => {
    state.features = { apps: true, chat: true, dockerPage: true };
    state.debugEnabled = true;
    state.focusChatIfOpen.mockReset().mockResolvedValue(false);
    state.focusDebugIfOpen.mockReset().mockResolvedValue(false);
});
afterEach(cleanup);

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}

describe('main window navigation', () => {
    it('keeps the outgoing page mounted until exit, then uses the latest target', () => {
        const { result } = renderHook(useAppNavigation);
        act(() => result.current.navigate('bots'));
        expect(result.current).toMatchObject({
            route: 'bots',
            displayedRoute: 'overview',
            pageVisible: false,
            direction: 1,
        });
        act(() => result.current.navigate('settings'));
        act(() => result.current.onPageExited());
        expect(result.current).toMatchObject({
            route: 'settings',
            displayedRoute: 'settings',
            pageVisible: true,
        });
        act(() => result.current.navigate('overview'));
        expect(result.current.direction).toBe(-1);
    });

    it('returns from a disabled page and prevents preloading or entering it', async () => {
        const { result, rerender } = renderHook(useAppNavigation);
        await act(async () => result.current.navigate('chat'));
        act(() => result.current.onPageExited());
        state.features = { ...state.features, chat: false };
        rerender();
        expect(result.current.route).toBe('overview');
        expect(result.current.pageVisible).toBe(false);
        act(() => result.current.onPageExited());
        act(() => {
            result.current.navigate('chat');
            result.current.prefetchRoute('chat');
        });
        expect(result.current.displayedRoute).toBe('overview');
        expect(state.preload).not.toHaveBeenCalled();
    });

    it.each(['chat', 'debug'] as const)(
        'focuses the %s popout without mounting another workspace',
        async (route) => {
            state.focusChatIfOpen.mockResolvedValue(true);
            state.focusDebugIfOpen.mockResolvedValue(true);
            const { result } = renderHook(useAppNavigation);
            await act(async () => result.current.navigate(route));
            expect(result.current).toMatchObject({
                route: 'overview',
                displayedRoute: 'overview',
                pageVisible: true,
            });
        },
    );

    it.each(['chat', 'debug'] as const)(
        'ignores a late %s window probe after a newer navigation',
        async (route) => {
            const pending = deferred<boolean>();
            state.focusChatIfOpen.mockReturnValue(pending.promise);
            state.focusDebugIfOpen.mockReturnValue(pending.promise);
            const { result } = renderHook(useAppNavigation);
            act(() => result.current.navigate(route));
            act(() => result.current.navigate('bots'));
            await act(async () => pending.resolve(false));
            expect(result.current.route).toBe('bots');
        },
    );

    it('finishes the latest navigation when handing off an outgoing chat page', () => {
        const { result } = renderHook(useAppNavigation);
        act(() => state.window!.showChat());
        act(() => result.current.navigate('bots'));
        expect(result.current.displayedRoute).toBe('chat');
        let restore: (() => void) | undefined;
        act(() => {
            restore = state.window!.releaseChat();
        });
        expect(result.current).toMatchObject({
            route: 'bots',
            displayedRoute: 'bots',
            pageVisible: true,
        });
        expect(restore).toBeUndefined();
    });

    it('releases chat entered while handoff is preparing and restores it only until a newer navigation', () => {
        const { result } = renderHook(useAppNavigation);
        const release = state.window!.releaseChat;
        act(() => state.window!.showChat());
        let restore: (() => void) | undefined;
        act(() => {
            restore = release();
        });
        expect(result.current.displayedRoute).toBe('overview');
        act(() => restore!());
        expect(result.current.displayedRoute).toBe('chat');
        act(() => {
            restore = release();
        });
        act(() => result.current.navigate('settings'));
        act(() => result.current.onPageExited());
        act(() => restore!());
        expect(result.current).toMatchObject({ route: 'settings', displayedRoute: 'settings' });
    });

    it('keeps an allowed window navigation when a different feature changes', async () => {
        const pending = deferred<boolean>();
        state.focusChatIfOpen.mockReturnValue(pending.promise);
        const { result, rerender } = renderHook(useAppNavigation);
        act(() => result.current.navigate('chat'));
        state.features = { ...state.features, apps: false };
        rerender();
        await act(async () => pending.resolve(false));
        expect(result.current.route).toBe('chat');
    });

    it.each(['chat', 'debug'] as const)(
        'reports failed %s window probes and allows retry without opening two workspaces',
        async (route) => {
            const probe = route === 'chat' ? state.focusChatIfOpen : state.focusDebugIfOpen;
            probe.mockRejectedValueOnce(new Error('窗口状态暂不可用'));
            const { result } = renderHook(useAppNavigation);
            await act(async () => result.current.navigate(route));
            expect(result.current.route).toBe('overview');
            expect(state.error).toHaveBeenCalledWith(
                expect.objectContaining({ raw: '窗口状态暂不可用' }),
            );
            await act(async () => result.current.navigate(route));
            expect(result.current.route).toBe(route);
        },
    );

    it('does not let a pending window probe undo a window handoff', async () => {
        const pending = deferred<boolean>();
        state.focusDebugIfOpen.mockReturnValue(pending.promise);
        const { result } = renderHook(useAppNavigation);
        act(() => result.current.navigate('debug'));
        act(() => state.window!.showChat());
        await act(async () => pending.resolve(false));
        expect(result.current).toMatchObject({
            route: 'chat',
            displayedRoute: 'chat',
            pageVisible: true,
        });
    });

    it('does not commit a pending navigation after its feature is disabled', async () => {
        const pending = deferred<boolean>();
        state.focusDebugIfOpen.mockReturnValue(pending.promise);
        const { result, rerender } = renderHook(useAppNavigation);
        act(() => result.current.navigate('debug'));
        state.debugEnabled = false;
        rerender();
        await act(async () => pending.resolve(false));
        expect(result.current.route).toBe('overview');
    });
});
