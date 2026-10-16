import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultAppUiPreferencesFromPrefs } from '../domain/settings/ui-preferences-bridge';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { defaultNetworkSettings } from '../domain/settings/network';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/transport', () => ({ isTauri: true, invoke: native.invoke }));

beforeEach(() => {
    native.invoke.mockReset();
});

function legacySettingsDto() {
    return {
        settings: {
            poller: {
                botLoginCheckInterval: 5000,
                botOfflineWebHookNotice: false,
                botOfflineEmailNotice: false,
            },
            mcp: { enabled: false, port: 0, allowDangerous: false },
            uiPreferences: defaultAppUiPreferencesFromPrefs(preferencesStore.get()),
        },
        githubPat: '',
    };
}

describe('GitHub API configuration', () => {
    it('keeps the proxy as the default when older settings have no API mode', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue(legacySettingsDto());

        expect((await settingsService.get()).githubApiMode).toBe('proxy');
    });

    it('saves and restores direct mode with a trimmed PAT outside app settings', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue(legacySettingsDto());
        const settings = await settingsService.get();
        settings.githubApiMode = 'direct';
        settings.githubPat = '  github_pat_test  ';

        await settingsService.set(settings);

        const saved = native.invoke.mock.calls.find(([name]) => name === 'set_app_settings');
        expect(saved?.[1]).toMatchObject({
            dto: { settings: { githubApiMode: 'direct' }, githubPat: 'github_pat_test' },
        });
        expect(saved?.[1].dto.settings).not.toHaveProperty('githubPat');
        native.invoke.mockResolvedValue(saved?.[1].dto);
        expect(await settingsService.get()).toMatchObject({
            githubApiMode: 'direct',
            githubPat: 'github_pat_test',
        });
    });

    it('persists switching back to proxy without discarding the stored PAT', async () => {
        const { settingsService } = await import('./settings.service');
        const dto = legacySettingsDto();
        native.invoke.mockResolvedValue({
            ...dto,
            settings: { ...dto.settings, githubApiMode: 'direct' },
            githubPat: 'github_pat_test',
        });
        const settings = await settingsService.get();
        settings.githubApiMode = 'proxy';

        await settingsService.set(settings);

        expect(native.invoke).toHaveBeenCalledWith('set_app_settings', {
            dto: expect.objectContaining({
                settings: expect.objectContaining({ githubApiMode: 'proxy' }),
                githubPat: 'github_pat_test',
            }),
        });
    });
});

describe('network configuration persistence', () => {
    it('supplies network defaults for settings saved before network configuration existed', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue(legacySettingsDto());
        expect(await settingsService.get()).toMatchObject({
            network: defaultNetworkSettings(),
            proxyPassword: '',
        });
    });

    it('round trips proxy credentials separately from serializable app settings', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue(legacySettingsDto());
        const settings = await settingsService.get();
        settings.network = {
            ...settings.network,
            proxyMode: 'manual',
            proxyUrl: 'socks5h://localhost:1080',
            proxyUsername: 'user',
            githubDownloadMode: 'mirror',
            nodeDownloadMode: 'direct',
        };
        settings.proxyPassword = ' secret with spaces ';
        await settingsService.set(settings);
        const saved = native.invoke.mock.calls.find(([name]) => name === 'set_app_settings')?.[1]
            .dto;
        expect(saved.settings.network).toEqual(settings.network);
        expect(saved.settings.network).not.toHaveProperty('proxyPassword');
        expect(saved.settings).not.toHaveProperty('proxyPassword');
        expect(saved.proxyPassword).toBe(' secret with spaces ');
        native.invoke.mockResolvedValue(saved);
        expect(await settingsService.get()).toMatchObject({
            network: settings.network,
            proxyPassword: settings.proxyPassword,
        });
    });

    it('blocks invalid network settings before IPC save', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue(legacySettingsDto());
        const settings = await settingsService.get();
        settings.network.proxyMode = 'manual';
        settings.network.proxyUrl = 'http://user:secret@proxy.example.com';
        await expect(settingsService.set(settings)).rejects.toThrow('地址中不能包含用户名或密码');
        expect(native.invoke.mock.calls.some(([name]) => name === 'set_app_settings')).toBe(false);
    });
});

describe('restoring persisted appearance', () => {
    it('keeps relogin opt-in for old settings and preserves its administrator scope on save', async () => {
        const { settingsService } = await import('./settings.service');
        native.invoke.mockResolvedValue({
            settings: {
                poller: {
                    botLoginCheckInterval: 5000,
                    botOfflineWebHookNotice: false,
                    botOfflineEmailNotice: false,
                },
                mcp: { enabled: false, port: 0, allowDangerous: false },
                uiPreferences: defaultAppUiPreferencesFromPrefs(preferencesStore.get()),
            },
            githubPat: '',
        });
        const settings = await settingsService.get();
        expect(settings.onebotRelogin).toEqual({
            enabled: false,
            administratorIds: [],
            botIds: [],
            offlineGraceSeconds: 60,
            refreshCooldownSeconds: 30,
        });
        settings.onebotRelogin = {
            ...settings.onebotRelogin,
            enabled: true,
            administratorIds: [12345],
            botIds: ['99'],
        };
        await settingsService.set(settings);
        const saved = native.invoke.mock.calls.find(([name]) => name === 'set_app_settings');
        expect(saved?.[1]).toMatchObject({
            dto: { settings: { onebotOfflineNotice: { relogin: settings.onebotRelogin } } },
        });
    });
    it('restores explicit default appearance over old local preferences after import', async () => {
        const { settingsService } = await import('./settings.service');
        const { hydrateAppUiPreferencesFromDisk } =
            await import('../../hooks/preferences/useAppUiPreferencesBootstrap');
        const original = preferencesStore.get();
        const defaults = {
            ...original,
            theme: 'auto' as const,
            showMascot: true,
            motionEnabled: true,
            motionLevel: 'standard' as const,
            motionSpeed: 0.5,
            radiusStyle: 'standard' as const,
        };
        preferencesStore.applySnapshot({ ...defaults, theme: 'dark', showMascot: false });
        native.invoke.mockResolvedValue({
            settings: {
                poller: {
                    botLoginCheckInterval: 5000,
                    botOfflineWebHookNotice: false,
                    botOfflineEmailNotice: false,
                },
                performanceMonitorEnabled: true,
                performanceMonitorInterval: 1200,
                uiPreferences: defaultAppUiPreferencesFromPrefs(defaults),
                mcp: { enabled: false, port: 0, allowDangerous: false },
            },
            githubPat: '',
        });

        try {
            await hydrateAppUiPreferencesFromDisk(true);
            expect(preferencesStore.get()).toMatchObject({ theme: 'auto', showMascot: true });
            expect((await settingsService.get()).uiPreferences).toMatchObject({
                theme: 'auto',
                showMascot: true,
            });
        } finally {
            preferencesStore.applySnapshot(original);
        }
    });
});
