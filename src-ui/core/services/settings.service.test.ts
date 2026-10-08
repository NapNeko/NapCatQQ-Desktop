import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultAppUiPreferencesFromPrefs } from '../domain/settings/ui-preferences-bridge';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/transport', () => ({ isTauri: true, invoke: native.invoke }));

beforeEach(() => {
    native.invoke.mockReset();
});

describe('restoring persisted appearance', () => {
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
