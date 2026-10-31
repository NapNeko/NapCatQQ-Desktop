import { afterEach, describe, expect, it, vi } from 'vitest';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import { onFileDragDrop, openExternalUrl } from './transport';

const native = vi.hoisted(() => ({ onDragDropEvent: vi.fn() }));
const bridge = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: true }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: bridge.invoke, Channel: class {} }));
vi.mock('../domain/runtime/env', () => ({
    get isTauri() {
        return bridge.isTauri;
    },
}));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => native }));

afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    bridge.invoke.mockReset();
    bridge.isTauri = true;
});

describe('external links', () => {
    it('routes web links through the host browser command and propagates failures', async () => {
        await openExternalUrl('https://github.com/NapNeko/NapCatQQ-Desktop');
        expect(bridge.invoke).toHaveBeenCalledWith('open_external_url', {
            url: 'https://github.com/NapNeko/NapCatQQ-Desktop',
        });
        bridge.invoke.mockRejectedValueOnce(new Error('browser unavailable'));
        await expect(openExternalUrl('http://127.0.0.1:6099')).rejects.toThrow(
            'browser unavailable',
        );
    });

    it.each(['not a url', 'file:///etc/passwd', 'javascript:alert(1)', 'data:text/html,hello'])(
        'rejects %s without native dispatch',
        async (url) => {
            await expect(openExternalUrl(url)).rejects.toThrow();
            expect(bridge.invoke).not.toHaveBeenCalled();
        },
    );

    it('opens browser preview links without IPC', async () => {
        bridge.isTauri = false;
        const open = vi.spyOn(window, 'open').mockReturnValue(null);
        await openExternalUrl('https://example.com');
        expect(open).toHaveBeenCalledWith('https://example.com/', '_blank', 'noopener,noreferrer');
        expect(bridge.invoke).not.toHaveBeenCalled();
    });
});

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
