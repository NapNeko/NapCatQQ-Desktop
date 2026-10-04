import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useConfigTransfer } from './useConfigTransfer';
import { configTransferService } from '../../core/services/config-transfer.service';

afterEach(() => vi.restoreAllMocks());

describe('preference restore retry', () => {
    it('keeps failed preferences available after leaving and returning to the data tab', () => {
        localStorage.clear();
        const client = new QueryClient();
        const wrapper = ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
        const first = renderHook(() => useConfigTransfer(), { wrapper });
        act(() => first.result.current.onImported({
            files: ['应用设置', '界面与终端偏好'], skipped: [],
            frontend_preferences: { version: 1, storage: { 'ncd.terminal.prefs.v1': '{"fontSize":19}' } },
            frontendPreferencesError: 'Storage full',
        }));
        first.unmount();
        const next = renderHook(() => useConfigTransfer(), { wrapper });
        expect(next.result.current.canRetryPreferences).toBe(true);
        act(() => next.result.current.retryPreferences());
        expect(next.result.current.canRetryPreferences).toBe(false);
        expect(localStorage.getItem('ncd.terminal.prefs.v1')).toBe('{"fontSize":19}');
        next.unmount();
        client.clear();
    });
});

describe('framework restore retry', () => {
    it('reloads persisted pending instances after reopening settings and retries independently', async () => {
        const pending = vi.spyOn(configTransferService, 'pendingFrameworkConfigs').mockResolvedValue(['麦麦']);
        const retry = vi.spyOn(configTransferService, 'retryFrameworkConfigs').mockResolvedValue({ files: ['框架配置 (mai-a)'], skipped: [], framework_pending: [] });
        const importConfig = vi.spyOn(configTransferService, 'import');
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const wrapper = ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
        const first = renderHook(() => useConfigTransfer(), { wrapper });
        await waitFor(() => expect(first.result.current.pendingFrameworks).toEqual(['麦麦']));
        first.unmount();
        const next = renderHook(() => useConfigTransfer(), { wrapper });
        expect(next.result.current.pendingFrameworks).toEqual(['麦麦']);
        pending.mockResolvedValue([]);
        act(() => next.result.current.retryFrameworks());
        await waitFor(() => expect(next.result.current.pendingFrameworks).toEqual([]));
        expect(retry).toHaveBeenCalledOnce(); expect(importConfig).not.toHaveBeenCalled();
        next.unmount(); client.clear();
    });

    it('retains pending recovery when the retry fails', async () => {
        vi.spyOn(configTransferService, 'pendingFrameworkConfigs').mockResolvedValue(['Karin']);
        vi.spyOn(configTransferService, 'retryFrameworkConfigs').mockRejectedValue(new Error('SSH disconnected'));
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const wrapper = ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
        const hook = renderHook(() => useConfigTransfer(), { wrapper });
        await waitFor(() => expect(hook.result.current.pendingFrameworks).toEqual(['Karin']));
        act(() => hook.result.current.retryFrameworks());
        await waitFor(() => expect(hook.result.current.isRestoringFrameworks).toBe(false));
        expect(hook.result.current.pendingFrameworks).toEqual(['Karin']);
        hook.unmount(); client.clear();
    });
});
