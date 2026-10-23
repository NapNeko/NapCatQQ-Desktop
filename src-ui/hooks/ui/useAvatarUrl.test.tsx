// @vitest-environment jsdom
import { act, cleanup, fireEvent, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AVATAR_REFRESH_MS, useAvatarUrl } from './useAvatarUrl';

const START = AVATAR_REFRESH_MS * 10;
const URL = 'https://q.qlogo.cn/headimg_dl?dst_uin=12345&spec=640';

beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(START);
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
});

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
});

describe('useAvatarUrl', () => {
    it('keeps a stable URL within the cache period and refreshes without a parent render', () => {
        const { result, rerender } = renderHook(() => useAvatarUrl(URL));
        const initial = result.current;
        expect(initial).toBe(`${URL}&_ncdAvatar=10`);

        act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS - 1));
        rerender();
        expect(result.current).toBe(initial);

        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(`${URL}&_ncdAvatar=11`);
    });

    it('shares one clock and removes the clock and listeners after the last avatar unmounts', () => {
        const removeDocumentListener = vi.spyOn(document, 'removeEventListener');
        const removeWindowListener = vi.spyOn(window, 'removeEventListener');
        const first = renderHook(() => useAvatarUrl(URL));
        const second = renderHook(() => useAvatarUrl('https://p.qlogo.cn/gh/20001/20001/640'));
        expect(second.result.current).toBe('https://p.qlogo.cn/gh/20001/20001/640?_ncdAvatar=10');
        expect(vi.getTimerCount()).toBe(1);

        first.unmount();
        expect(vi.getTimerCount()).toBe(1);
        act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS));
        expect(second.result.current).toBe('https://p.qlogo.cn/gh/20001/20001/640?_ncdAvatar=11');

        second.unmount();
        expect(vi.getTimerCount()).toBe(0);
        expect(removeDocumentListener).toHaveBeenCalledWith(
            'visibilitychange',
            expect.any(Function),
        );
        expect(removeWindowListener).toHaveBeenCalledWith('focus', expect.any(Function));
        fireEvent(window, new Event('focus'));
        fireEvent(document, new Event('visibilitychange'));
        expect(vi.getTimerCount()).toBe(0);
    });

    it.each(['visibilitychange', 'focus'])(
        'refreshes expired portraits on %s after a hidden period',
        (event) => {
            let visibility: DocumentVisibilityState = 'visible';
            vi.spyOn(document, 'visibilityState', 'get').mockImplementation(() => visibility);
            const { result } = renderHook(() => useAvatarUrl(URL));
            const initial = result.current;

            visibility = 'hidden';
            fireEvent(document, new Event('visibilitychange'));
            expect(vi.getTimerCount()).toBe(0);
            act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS * 3));
            expect(result.current).toBe(initial);

            visibility = 'visible';
            fireEvent(event === 'focus' ? window : document, new Event(event));
            expect(result.current).toBe(`${URL}&_ncdAvatar=13`);
            expect(vi.getTimerCount()).toBe(1);
        },
    );

    it('does not start a refresh clock without a portrait URL', () => {
        const { result } = renderHook(() => useAvatarUrl(null));
        expect(result.current).toBeNull();
        expect(vi.getTimerCount()).toBe(0);
    });
});
