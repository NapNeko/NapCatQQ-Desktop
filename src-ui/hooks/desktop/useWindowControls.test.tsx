import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useWindowControls, WindowControlsProvider } from './useWindowControls';

const service = vi.hoisted(() => ({
    frameState: vi.fn(),
    onResize: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
}));
vi.mock('../../core/services/desktop.service', () => ({ windowControlService: service }));

describe('window controls subscriptions', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        service.frameState.mockResolvedValue({
            isMaximized: false,
            isFullscreen: false,
            isResizable: true,
        });
    });

    it('releases a resize listener which arrives after the title bar unmounts', async () => {
        let resolve!: (stop: () => void) => void;
        service.onResize.mockReturnValueOnce(
            new Promise<() => void>((done) => {
                resolve = done;
            }),
        );
        const stop = vi.fn();
        const hook = renderHook(useWindowControls, { wrapper: WindowControlsProvider });
        await waitFor(() => expect(service.onResize).toHaveBeenCalledOnce());
        hook.unmount();
        await act(async () => {
            resolve(stop);
        });
        expect(stop).toHaveBeenCalledOnce();
    });

    it('coalesces native resize events into one state refresh', async () => {
        let resize!: () => void;
        service.onResize.mockImplementationOnce(async (cb: () => void) => {
            resize = cb;
            return vi.fn();
        });
        const hook = renderHook(useWindowControls, { wrapper: WindowControlsProvider });
        await waitFor(() => expect(hook.result.current.ready).toBe(true));
        vi.useFakeTimers();
        try {
            resize();
            resize();
            resize();
            expect(service.frameState).toHaveBeenCalledOnce();
            await act(async () => vi.advanceTimersByTime(60));
            expect(service.frameState).toHaveBeenCalledTimes(2);
        } finally {
            hook.unmount();
            vi.useRealTimers();
        }
    });
});
