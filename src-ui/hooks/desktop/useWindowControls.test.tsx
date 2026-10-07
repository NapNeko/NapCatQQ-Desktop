import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useWindowControls } from './useWindowControls';

const service = vi.hoisted(() => ({
    isMaximized: vi.fn(),
    onResize: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
}));
vi.mock('../../core/services/desktop.service', () => ({ windowControlService: service }));

describe('window controls subscriptions', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        service.isMaximized.mockResolvedValue(false);
    });

    it('releases a resize listener which arrives after the title bar unmounts', async () => {
        let resolve!: (stop: () => void) => void;
        service.onResize.mockReturnValueOnce(
            new Promise<() => void>((done) => {
                resolve = done;
            }),
        );
        const stop = vi.fn();
        const hook = renderHook(useWindowControls);
        await waitFor(() => expect(service.onResize).toHaveBeenCalledOnce());
        hook.unmount();
        await act(async () => {
            resolve(stop);
        });
        expect(stop).toHaveBeenCalledOnce();
    });
});
