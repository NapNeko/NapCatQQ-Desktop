import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useHistoryPaging } from './useHistoryPaging';

describe('upward history paging', () => {
    const settlePaging = async () => {
        await act(async () => {
            await new Promise<void>((resolve) => {
                requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
            });
        });
    };

    it('loads only from upward intent near the top, with one request in flight', async () => {
        const el = document.createElement('div');
        el.scrollTop = 50;
        const detach = vi.fn();
        let finish!: () => void;
        const load = vi.fn(
            () =>
                new Promise<void>((resolve) => {
                    finish = resolve;
                }),
        );
        const { result } = renderHook(() =>
            useHistoryPaging({ scroll: { current: el }, enabled: true, load, detach }),
        );
        act(() => result.current.onScroll());
        expect(load).not.toHaveBeenCalled();
        act(() => result.current.onWheel({ deltaY: -10 }));
        act(() => result.current.onWheel({ deltaY: -10 }));
        el.scrollTop = 0;
        act(() => result.current.onScroll());
        await settlePaging();
        expect(load).toHaveBeenCalledTimes(1);
        expect(detach).toHaveBeenCalledTimes(1);
        await act(async () => {
            finish();
        });
        act(() => result.current.onWheel({ deltaY: 10 }));
        expect(load).toHaveBeenCalledTimes(1);
    });
    it('waits for scrolling into the threshold and supports Home without a wheel', async () => {
        const el = document.createElement('div');
        el.scrollTop = 500;
        const load = vi.fn(async () => {});
        const { result } = renderHook(() =>
            useHistoryPaging({ scroll: { current: el }, enabled: true, load, detach: vi.fn() }),
        );
        act(() => result.current.onKeyDown({ key: 'Home' }));
        expect(load).not.toHaveBeenCalled();
        el.scrollTop = 0;
        act(() => result.current.onScroll());
        await settlePaging();
        expect(load).toHaveBeenCalledTimes(1);
    });
    it('does not automatically retry a failed page or fetch after exhaustion', () => {
        const el = document.createElement('div');
        const load = vi.fn(async () => {});
        const { result } = renderHook(() =>
            useHistoryPaging({ scroll: { current: el }, enabled: false, load, detach: vi.fn() }),
        );
        act(() => {
            result.current.onWheel({ deltaY: -20 });
            result.current.onScroll();
        });
        expect(load).not.toHaveBeenCalled();
    });
    it('prefetches before the top and can load from a wheel while already at the boundary', async () => {
        const el = document.createElement('div');
        el.scrollTop = 200;
        const load = vi.fn(async () => {});
        const { result } = renderHook(() =>
            useHistoryPaging({ scroll: { current: el }, enabled: true, load, detach: vi.fn() }),
        );
        act(() => result.current.onWheel({ deltaY: -100 }));
        await settlePaging();
        expect(load).toHaveBeenCalledTimes(1);
        el.scrollTop = 0;
        act(() => result.current.onWheel({ deltaY: -100 }));
        await settlePaging();
        expect(load).toHaveBeenCalledTimes(2);
    });
});
