// @vitest-environment jsdom
import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { dismissInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { appFrameworkVersionsKey, useAppFrameworkVersions } from './useAppFrameworkVersions';

vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: { listVersions: vi.fn() },
}));
vi.mock('../ui/globalInfoBarStore', () => ({ dismissInfoBar: vi.fn() }));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

it('版本刷新失败发全局提示并保留缓存，重试成功清除提示', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const versions = { latest: '1.0.0', versions: ['1.0.0'], has_prerelease: false };
    client.setQueryData(appFrameworkVersionsKey('neobot'), versions);
    vi.mocked(appFrameworkService.listVersions).mockRejectedValueOnce(new Error('版本源不可达'));
    const { result, rerender } = renderHook(() => useAppFrameworkVersions('neobot'), {
        wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
    });
    await act(async () => {
        await result.current.refetch();
    });
    await waitFor(() =>
        expect(pushErrorBar).toHaveBeenCalledWith({
            key: 'app-versions:neobot',
            title: '读取可用版本失败',
            raw: '版本源不可达',
        }),
    );
    expect(result.current.data).toEqual(versions);
    rerender();
    expect(pushErrorBar).toHaveBeenCalledTimes(1);
    vi.mocked(dismissInfoBar).mockClear();
    vi.mocked(appFrameworkService.listVersions).mockResolvedValueOnce(versions);
    await act(async () => {
        await result.current.refetch();
    });
    await waitFor(() => expect(dismissInfoBar).toHaveBeenCalledWith('key:app-versions:neobot'));
    client.clear();
});
