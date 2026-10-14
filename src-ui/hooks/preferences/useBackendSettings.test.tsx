import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { mockBackendSettings } from '../../core/ipc/mock/settings.mock';
import { settingsService } from '../../core/services/settings.service';
import { draftFromBackendAndPrefs } from '../../modules/settings/settings-draft';
import { pushErrorBar } from '../ui/pushErrorBar';
import { useBackendSettings } from './useBackendSettings';

vi.mock('../../modules/settings/settings-draft', async (importOriginal) => ({
    ...(await importOriginal<typeof import('../../modules/settings/settings-draft')>()),
    applyClientPrefsFromDraft: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('../bot/botRuntimeMetricsCatalog', () => ({
    reloadBotRuntimeMetricsCatalogSettings: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: vi.fn() }));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

afterEach(() => vi.restoreAllMocks());

function mountSettings() {
    const baseline = { ...mockBackendSettings, githubApiMode: 'proxy' as const, githubPat: '' };
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    client.setQueryData(['appSettings'], baseline);
    client.setQueryData(['releaseSnapshot'], { source: 'previous-settings' });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const hook = renderHook(() => useBackendSettings(), { wrapper });
    return { baseline, client, hook };
}

describe('GitHub settings save', () => {
    it('restarts an initial release request and ignores its old-route result after saving', async () => {
        vi.spyOn(settingsService, 'set').mockResolvedValue(undefined);
        const baseline = { ...mockBackendSettings, githubApiMode: 'proxy' as const, githubPat: '' };
        const client = new QueryClient({
            defaultOptions: { queries: { retry: false, staleTime: Infinity } },
        });
        client.setQueryData(['appSettings'], baseline);
        let resolveProxy!: (value: { source: string }) => void;
        let resolveDirect!: (value: { source: string }) => void;
        const proxyRequest = new Promise<{ source: string }>((resolve) => {
            resolveProxy = resolve;
        });
        const directRequest = new Promise<{ source: string }>((resolve) => {
            resolveDirect = resolve;
        });
        const fetchReleases = vi.fn(() =>
            client.getQueryData<typeof baseline>(['appSettings'])?.githubApiMode === 'proxy'
                ? proxyRequest
                : directRequest,
        );
        const wrapper = ({ children }: { children: React.ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        );
        const hook = renderHook(
            () => ({
                settings: useBackendSettings(),
                releases: useQuery({ queryKey: ['releaseSnapshot'], queryFn: fetchReleases }),
            }),
            { wrapper },
        );
        await waitFor(() => expect(fetchReleases).toHaveBeenCalledOnce());
        expect(hook.result.current.releases.data).toBeUndefined();
        act(() =>
            hook.result.current.settings.save({
                ...draftFromBackendAndPrefs(baseline),
                githubApiMode: 'direct',
            }),
        );
        await waitFor(() => expect(fetchReleases).toHaveBeenCalledTimes(2));
        await act(async () => {
            resolveDirect({ source: 'direct' });
            await directRequest;
        });
        await waitFor(() =>
            expect(hook.result.current.releases.data).toEqual({ source: 'direct' }),
        );
        await act(async () => {
            resolveProxy({ source: 'proxy' });
            await proxyRequest;
        });
        expect(client.getQueryData(['releaseSnapshot'])).toEqual({ source: 'direct' });
        expect(hook.result.current.releases.data).toEqual({ source: 'direct' });
        hook.unmount();
        client.clear();
    });

    it.each([{ githubApiMode: 'direct' as const }, { githubPat: 'github_pat_example' }])(
        'refreshes cached releases after saving %o',
        async (patch) => {
            const save = vi.spyOn(settingsService, 'set').mockResolvedValue(undefined);
            const { baseline, client, hook } = mountSettings();
            const draft = { ...draftFromBackendAndPrefs(baseline), ...patch };
            act(() => hook.result.current.save(draft));
            await waitFor(() => expect(hook.result.current.settings).toMatchObject(patch));
            expect(save).toHaveBeenCalledWith(expect.objectContaining(patch));
            expect(client.getQueryState(['releaseSnapshot'])?.isInvalidated).toBe(true);
            expect(hook.result.current.settings).toMatchObject(patch);
            hook.unmount();
            client.clear();
        },
    );

    it('keeps the release cache when GitHub configuration is unchanged', async () => {
        vi.spyOn(settingsService, 'set').mockResolvedValue(undefined);
        const { baseline, client, hook } = mountSettings();
        const draft = {
            ...draftFromBackendAndPrefs(baseline),
            botLoginCheckIntervalMs: baseline.botLoginCheckIntervalMs + 1000,
        };
        act(() => hook.result.current.save(draft));
        await waitFor(() =>
            expect(hook.result.current.settings?.botLoginCheckIntervalMs).toBe(
                draft.botLoginCheckIntervalMs,
            ),
        );
        expect(client.getQueryState(['releaseSnapshot'])?.isInvalidated).toBe(false);
        hook.unmount();
        client.clear();
    });

    it('keeps saved settings and releases when persisting GitHub configuration fails', async () => {
        vi.spyOn(settingsService, 'set').mockRejectedValue(new Error('Keyring unavailable'));
        const { baseline, client, hook } = mountSettings();
        act(() =>
            hook.result.current.save({
                ...draftFromBackendAndPrefs(baseline),
                githubApiMode: 'direct',
                githubPat: 'github_pat_example',
            }),
        );
        await waitFor(() => expect(pushErrorBar).toHaveBeenCalled());
        expect(hook.result.current.settings).toEqual(baseline);
        expect(client.getQueryState(['releaseSnapshot'])?.isInvalidated).toBe(false);
        hook.unmount();
        client.clear();
    });
});
