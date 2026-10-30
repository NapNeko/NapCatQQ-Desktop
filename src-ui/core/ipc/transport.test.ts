import { afterEach, describe, expect, it, vi } from 'vitest';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { onFileDragDrop } from './transport';

const native = vi.hoisted(() => ({ onDragDropEvent: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => native }));

afterEach(() => vi.unstubAllGlobals());

describe('native file drop coordinates', () => {
    it('keeps the same CSS target when page zoom and monitor DPI change', async () => {
        const stop = vi.fn();
        native.onDragDropEvent.mockResolvedValue(stop);
        const handler = vi.fn();
        expect(await onFileDragDrop(handler)).toBe(stop);
        const receive = native.onDragDropEvent.mock.calls[0][0];

        vi.stubGlobal('devicePixelRatio', 1.25);
        receive({ payload: { type: 'over', position: new PhysicalPosition(100, 50) } });
        vi.stubGlobal('devicePixelRatio', 2.5);
        receive({ payload: { type: 'over', position: new PhysicalPosition(200, 100) } });

        expect(handler.mock.calls.map(([event]) => event)).toEqual([
            { type: 'over', position: { x: 80, y: 40 } },
            { type: 'over', position: { x: 80, y: 40 } },
        ]);
    });

    it('preserves paths and leave events at 150% page zoom', async () => {
        native.onDragDropEvent.mockResolvedValue(vi.fn());
        const handler = vi.fn();
        await onFileDragDrop(handler);
        const receive = native.onDragDropEvent.mock.calls[0][0];
        vi.stubGlobal('devicePixelRatio', 1.5);
        for (const type of ['enter', 'drop']) {
            receive({
                payload: { type, paths: ['/tmp/file.txt'], position: new PhysicalPosition(90, 60) },
            });
        }
        receive({ payload: { type: 'leave' } });

        expect(handler.mock.calls.map(([event]) => event)).toEqual([
            { type: 'enter', paths: ['/tmp/file.txt'], position: { x: 60, y: 40 } },
            { type: 'drop', paths: ['/tmp/file.txt'], position: { x: 60, y: 40 } },
            { type: 'leave' },
        ]);
    });
});
