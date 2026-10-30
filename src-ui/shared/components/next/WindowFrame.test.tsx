import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WindowFrame } from './WindowFrame';
import { TitleBarChrome } from './TitleBarChrome';
import { Dialog, DialogContent, DialogTitle } from '../../ui/Dialog';

const service = vi.hoisted(() => ({
    frameState: vi.fn(),
    onResize: vi.fn(),
    startResizeDragging: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    closeSelf: vi.fn(),
}));
vi.mock('../../../core/services/desktop.service', () => ({ windowControlService: service }));
vi.mock('../../../core/domain/runtime/env', () => ({ isTauri: true, localOs: () => 'linux' }));

const normal = { isMaximized: false, isFullscreen: false, isResizable: true };

beforeEach(() => {
    service.frameState.mockResolvedValue(normal);
    service.onResize.mockResolvedValue(vi.fn());
    service.startResizeDragging.mockResolvedValue(undefined);
});
afterEach(cleanup);

describe('native window frame', () => {
    it('shares one state subscription and sends only primary presses to native resize', async () => {
        render(
            <WindowFrame>
                <TitleBarChrome />
            </WindowFrame>,
        );
        await waitFor(() =>
            expect(document.querySelectorAll('[data-window-resize]')).toHaveLength(8),
        );
        expect(service.onResize).toHaveBeenCalledOnce();
        expect(service.frameState).toHaveBeenCalledOnce();
        const corner = document.querySelector('[data-window-resize="SouthEast"]')!;
        fireEvent.mouseDown(corner, { button: 2 });
        expect(service.startResizeDragging).not.toHaveBeenCalled();
        fireEvent.mouseDown(corner, { button: 0 });
        expect(service.startResizeDragging).toHaveBeenCalledWith('SouthEast');
        expect(document.querySelector('header')).toHaveAttribute('data-tauri-drag-region', 'deep');
    });

    it.each([{ isMaximized: true }, { isFullscreen: true }, { isResizable: false }])(
        'disables edge dragging for window state %o',
        async (patch) => {
            service.frameState.mockResolvedValue({ ...normal, ...patch });
            await act(async () => {
                render(<WindowFrame>窗口</WindowFrame>);
            });
            expect(document.querySelector('[data-window-resize]')).toBeNull();
            expect(document.documentElement.hasAttribute('data-window-expanded')).toBe(
                !!(patch.isMaximized || patch.isFullscreen),
            );
        },
    );

    it('keeps a modal open when resizing from a body portal', async () => {
        const onOpenChange = vi.fn();
        render(
            <WindowFrame>
                <Dialog open onOpenChange={onOpenChange}>
                    <DialogContent>
                        <DialogTitle>调整窗口</DialogTitle>
                    </DialogContent>
                </Dialog>
            </WindowFrame>,
        );
        await screen.findByRole('dialog');
        await waitFor(() =>
            expect(document.querySelectorAll('[data-window-resize]')).toHaveLength(8),
        );
        expect(document.body.style.pointerEvents).toBe('none');
        const edge = document.querySelector('[data-window-resize="East"]')!;
        fireEvent.pointerDown(edge, { button: 0 });
        fireEvent.mouseDown(edge, { button: 0 });
        expect(service.startResizeDragging).toHaveBeenCalledWith('East');
        expect(onOpenChange).not.toHaveBeenCalled();
    });
});
