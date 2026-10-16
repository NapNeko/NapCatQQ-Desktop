import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultNetworkSettings } from '../domain/settings/network';
import { networkService } from './network.service';

const transport = vi.hoisted(() => ({ native: true, invoke: vi.fn() }));
vi.mock('../ipc/transport', () => ({
    get isTauri() {
        return transport.native;
    },
    invoke: transport.invoke,
}));

beforeEach(() => {
    transport.native = true;
    transport.invoke.mockReset();
});

describe('network diagnostics service', () => {
    it('submits the current settings without invoking the save command', async () => {
        transport.invoke.mockResolvedValue([]);
        const settings = defaultNetworkSettings();
        await networkService.test(settings, 'draft-password');
        expect(transport.invoke).toHaveBeenCalledTimes(1);
        expect(transport.invoke).toHaveBeenCalledWith('test_network_settings', {
            settings,
            proxyPassword: 'draft-password',
        });
    });

    it('does not pretend to test connectivity in browser preview', async () => {
        transport.native = false;
        await expect(networkService.test(defaultNetworkSettings(), '')).rejects.toThrow(
            '浏览器预览不执行真实网络测试',
        );
        expect(transport.invoke).not.toHaveBeenCalled();
        const mirrors = await networkService.listDownloadMirrors();
        expect(mirrors.map((mirror) => mirror.kind)).toEqual(['github', 'node']);
    });
});
