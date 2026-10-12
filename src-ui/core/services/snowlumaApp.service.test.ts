import { beforeEach, describe, expect, it, vi } from 'vitest';

const transport = vi.hoisted(() => ({ isTauri: true, invoke: vi.fn() }));
vi.mock('../ipc/transport', () => ({
    get isTauri() {
        return transport.isTauri;
    },
    invoke: transport.invoke,
}));
import { snowlumaAppService } from './snowlumaApp.service';

describe('SnowLuma QR config defaults', () => {
    beforeEach(() => {
        transport.isTauri = true;
        transport.invoke.mockReset();
    });

    it('enables missing QR flags returned by older config data', async () => {
        transport.invoke.mockResolvedValue({
            snowlumaWebuiPasswordOverride: '',
            snowlumaWebuiPort: 5099,
        });
        const config = await snowlumaAppService.get();
        expect(config.snowlumaRemoteQrLoginEnabled).toBe(true);
        expect(config.snowlumaLocalQrLoginEnabled).toBe(true);
    });

    it('preserves explicitly disabled QR flags', async () => {
        transport.invoke.mockResolvedValue({
            snowlumaWebuiPasswordOverride: '',
            snowlumaWebuiPort: 5099,
            snowlumaRemoteQrLoginEnabled: false,
            snowlumaLocalQrLoginEnabled: false,
        });
        const config = await snowlumaAppService.get();
        expect(config.snowlumaRemoteQrLoginEnabled).toBe(false);
        expect(config.snowlumaLocalQrLoginEnabled).toBe(false);
    });

    it('uses the same enabled defaults in the browser preview', async () => {
        transport.isTauri = false;
        const config = await snowlumaAppService.get();
        expect(config.snowlumaRemoteQrLoginEnabled).toBe(true);
        expect(config.snowlumaLocalQrLoginEnabled).toBe(true);
        expect(transport.invoke).not.toHaveBeenCalled();
    });
});
