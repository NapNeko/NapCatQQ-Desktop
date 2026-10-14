import { describe, expect, it } from 'vitest';
import { mockBackendSettings } from '../../core/ipc/mock/settings.mock';
import { backendSlice, draftFromBackendAndPrefs, isSettingsDirty } from './settings-draft';

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
