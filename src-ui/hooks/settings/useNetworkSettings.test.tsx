// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
    defaultNetworkSettings,
    type NetworkProbeResult,
} from '../../core/domain/settings/network';
import { useNetworkSettings } from './useNetworkSettings';

const service = vi.hoisted(() => ({ listDownloadMirrors: vi.fn(), test: vi.fn() }));
vi.mock('../../core/services/network.service', () => ({ networkService: service }));

const successful: NetworkProbeResult[] = [
    {
        target: 'GitHub',
        url: 'https://github.com',
        elapsedMs: 12,
        success: true,
        message: 'test fixture',
    },
];

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

beforeEach(() => {
    service.listDownloadMirrors.mockReset().mockResolvedValue([]);
    service.test.mockReset();
});
afterEach(cleanup);

describe('network draft diagnostics', () => {
    it('tests the current draft once and rejects concurrent attempts', async () => {
        const gate = deferred<NetworkProbeResult[]>();
        service.test.mockReturnValue(gate.promise);
        const settings = {
            ...defaultNetworkSettings(),
            proxyMode: 'manual' as const,
            proxyUrl: 'http://localhost:7890',
            proxyUsername: 'user',
        };
        const { result } = renderHook(() => useNetworkSettings(settings, ' draft password '));
        let request!: Promise<void>;
        act(() => {
            request = result.current.test();
            void result.current.test();
        });
        expect(result.current.busy).toBe(true);
        expect(service.test).toHaveBeenCalledTimes(1);
        expect(service.test).toHaveBeenCalledWith(settings, ' draft password ');
        expect(service.test.mock.calls[0][0]).not.toBe(settings);
        await act(async () => {
            gate.resolve(successful);
            await request;
        });
        expect(result.current.results).toEqual(successful);
        expect(result.current.busy).toBe(false);
    });

    it('discards late responses even after editing and reverting to the original draft', async () => {
        const gate = deferred<NetworkProbeResult[]>();
        service.test.mockReturnValue(gate.promise);
        const settings = defaultNetworkSettings();
        const { result, rerender } = renderHook(
            ({ password }) => useNetworkSettings(settings, password),
            { initialProps: { password: '' } },
        );
        let request!: Promise<void>;
        act(() => {
            request = result.current.test();
        });
        rerender({ password: 'changed' });
        rerender({ password: '' });
        await act(async () => {
            gate.resolve(successful);
            await request;
        });
        expect(result.current.results).toEqual([]);
        expect(result.current.error).toBeNull();
        expect(result.current.busy).toBe(false);
    });

    it('clears displayed results on route changes and discards late errors', async () => {
        service.test.mockResolvedValueOnce(successful);
        const { result, rerender } = renderHook(
            ({ settings }) => useNetworkSettings(settings, ''),
            { initialProps: { settings: defaultNetworkSettings() } },
        );
        await act(() => result.current.test());
        expect(result.current.results).toEqual(successful);
        rerender({ settings: { ...defaultNetworkSettings(), nodeDownloadMode: 'direct' } });
        expect(result.current.results).toEqual([]);
        const gate = deferred<NetworkProbeResult[]>();
        service.test.mockReturnValueOnce(gate.promise);
        let request!: Promise<void>;
        act(() => {
            request = result.current.test();
        });
        rerender({ settings: defaultNetworkSettings() });
        await act(async () => {
            gate.reject(new Error('stale failure'));
            await request;
        });
        expect(result.current.error).toBeNull();
    });
});
