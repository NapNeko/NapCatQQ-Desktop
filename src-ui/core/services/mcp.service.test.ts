// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mcpService } from './mcp.service';

const transport = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: true }));
vi.mock('../ipc/transport', () => ({
    invoke: transport.invoke,
    get isTauri() {
        return transport.isTauri;
    },
}));

describe('MCP credentials', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        transport.isTauri = true;
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: { writeText: vi.fn().mockResolvedValue(undefined) },
        });
    });

    it('reads no credential during status checks', async () => {
        transport.invoke.mockResolvedValue({
            enabled: true,
            listening: true,
            port: 12345,
            lastError: null,
        });
        await mcpService.status();
        expect(transport.invoke).toHaveBeenCalledTimes(1);
        expect(transport.invoke).toHaveBeenCalledWith('mcp_status');
        expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
    });

    it('copies a credential only on request and returns no secret to callers', async () => {
        transport.invoke.mockResolvedValue('{"Authorization":"Bearer test"}');
        expect(await mcpService.copyConnectionConfig()).toBeUndefined();
        expect(transport.invoke).toHaveBeenCalledTimes(1);
        expect(transport.invoke).toHaveBeenCalledWith('mcp_connection_config');
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
            '{"Authorization":"Bearer test"}',
        );
    });

    it('reports clipboard failure without success and rejects browser-only credential access', async () => {
        transport.invoke.mockResolvedValue('private config');
        vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(
            new Error('clipboard unavailable'),
        );
        await expect(mcpService.copyConnectionConfig()).rejects.toThrow('clipboard unavailable');
        vi.clearAllMocks();
        transport.isTauri = false;
        await expect(mcpService.copyConnectionConfig()).rejects.toThrow('桌面应用');
        expect(transport.invoke).not.toHaveBeenCalled();
    });
});
