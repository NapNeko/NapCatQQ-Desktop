import { describe, expect, it } from 'vitest';
import { mockBackendSettings } from '../../core/ipc/mock/settings.mock';
import { backendSlice, draftFromBackendAndPrefs, isSettingsDirty } from './settings-draft';
import { defaultNetworkSettings, type NetworkSettings } from '../../core/domain/settings/network';

describe('GitHub API settings draft', () => {
    it('marks a mode-only change as dirty and clears it when reverted', () => {
        const backend = { ...mockBackendSettings, githubApiMode: 'proxy' as const };
        const draft = draftFromBackendAndPrefs(backend);

        expect(draft.githubApiMode).toBe('proxy');
        expect(isSettingsDirty(draft, backend)).toBe(false);
        draft.githubApiMode = 'direct';
        expect(isSettingsDirty(draft, backend)).toBe(true);
        draft.githubApiMode = 'proxy';
        expect(isSettingsDirty(draft, backend)).toBe(false);
    });

    it('keeps the selected mode and PAT through the save and reload slices', () => {
        const backend = { ...mockBackendSettings, githubApiMode: 'proxy' as const };
        const draft = draftFromBackendAndPrefs(backend);
        draft.githubApiMode = 'direct';
        draft.githubPat = 'github_pat_test';

        const saved = backendSlice(draft);
        expect(saved).toMatchObject({
            githubApiMode: 'direct',
            githubPat: 'github_pat_test',
        });
        const restored = draftFromBackendAndPrefs(saved);
        expect(restored).toMatchObject({
            githubApiMode: 'direct',
            githubPat: 'github_pat_test',
        });
        expect(isSettingsDirty(restored, saved)).toBe(false);
    });
});

describe('network settings draft', () => {
    it.each([
        ['proxyMode', 'manual'],
        ['proxyUrl', 'socks5h://127.0.0.1:1080'],
        ['proxyUsername', 'proxy-user'],
        ['proxyBypass', 'localhost,.example.com,192.168.0.0/16'],
        ['githubDownloadMode', 'mirror'],
        ['githubMirror', 'https://mirror.example.com'],
        ['nodeDownloadMode', 'direct'],
        ['nodeMirror', 'https://mirror.example.com/node'],
    ] as Array<[keyof NetworkSettings, string]>)(
        'tracks and reverts %s without mutating saved settings',
        (key, value) => {
            const backend = { ...mockBackendSettings, network: defaultNetworkSettings() };
            const draft = draftFromBackendAndPrefs(backend);
            const original = backend.network[key];
            Object.assign(draft.network, { [key]: value });
            expect(backend.network[key]).toBe(original);
            expect(isSettingsDirty(draft, backend)).toBe(true);
            Object.assign(draft.network, { [key]: original });
            expect(isSettingsDirty(draft, backend)).toBe(false);
        },
    );

    it('keeps proxy authentication and both fixed routes through save and reload', () => {
        const backend = { ...mockBackendSettings, network: defaultNetworkSettings() };
        const draft = draftFromBackendAndPrefs(backend);
        draft.network = {
            ...draft.network,
            proxyMode: 'manual',
            proxyUrl: 'https://proxy.example.com',
            proxyUsername: 'alice',
            githubDownloadMode: 'mirror',
            githubMirror: 'https://mirror.example.com',
            nodeDownloadMode: 'direct',
        };
        draft.proxyPassword = ' password with spaces ';
        const saved = backendSlice(draft);
        expect(saved.network).toEqual(draft.network);
        expect(saved.network).not.toBe(draft.network);
        expect(saved.proxyPassword).toBe(draft.proxyPassword);
        const restored = draftFromBackendAndPrefs(saved);
        expect(restored.network).toEqual(draft.network);
        expect(restored.proxyPassword).toBe(draft.proxyPassword);
        expect(isSettingsDirty(restored, saved)).toBe(false);
        restored.proxyPassword = '';
        expect(isSettingsDirty(restored, saved)).toBe(true);
    });
});
