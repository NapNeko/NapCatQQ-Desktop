import type { ReactNode } from 'react';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useContainerEngine } from './useContainerEngine';

const service = vi.hoisted(() => ({ getEngine: vi.fn(), setEngine: vi.fn() }));
vi.mock('../../core/services/docker.service', () => ({ dockerService: service }));
afterEach(cleanup);
beforeEach(() => {
    service.getEngine.mockResolvedValue('docker');
    service.setEngine.mockReset().mockResolvedValue(undefined);
});
function setup() {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, gcTime: Infinity } },
    });
    client.setQueryData(['docker', 'containers', 'local'], ['old-engine-container']);
    client.setQueryData(['docker', 'containers', 'remote:other'], ['other-host-container']);
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    return { client, ...renderHook(() => useContainerEngine('local'), { wrapper }) };
}
describe('container engine selection', () => {
    it('persists the host choice and clears only that engine resource cache', async () => {
        const { result, client } = setup();
        await waitFor(() => expect(result.current.loading).toBe(false));
        await act(async () => result.current.select('podman'));
        expect(service.setEngine).toHaveBeenCalledWith('local', 'podman');
        await waitFor(() => expect(result.current.engine).toBe('podman'));
        expect(client.getQueryData(['docker', 'containers', 'local'])).toBeUndefined();
        expect(client.getQueryData(['docker', 'containers', 'remote:other'])).toEqual([
            'other-host-container',
        ]);
    });
    it('retains the current engine and resources when switching is refused', async () => {
        service.setEngine.mockRejectedValue(new Error('已有容器 Bot 配置'));
        const { result, client } = setup();
        await waitFor(() => expect(result.current.loading).toBe(false));
        await act(async () => {
            await expect(result.current.select('podman')).rejects.toThrow('已有容器 Bot 配置');
        });
        expect(result.current.engine).toBe('docker');
        expect(client.getQueryData(['docker', 'containers', 'local'])).toEqual([
            'old-engine-container',
        ]);
    });
});
