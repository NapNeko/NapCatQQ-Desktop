import { describe, expect, it } from 'vitest';
import {
    ASTRBOT_SOURCE_PRESETS,
    astrbotDefaultConfig,
    newOpenAiModel,
    newSourceFromPreset,
} from '../../../../core/domain/apps/astrbotConfig';
import {
    commitProviderDraft,
    draftForCreate,
    draftForEdit,
    removeProviderSource,
} from './providerDraft';
import type { AstrBotInstanceConfig } from '../../../../core/ipc/types';

const deepseek = ASTRBOT_SOURCE_PRESETS.find((p) => p.id === 'deepseek')!;
const openai = ASTRBOT_SOURCE_PRESETS.find((p) => p.id === 'openai')!;

function twoSources(): AstrBotInstanceConfig {
    const cfg = astrbotDefaultConfig(6199);
    const a = { ...newSourceFromPreset(openai, []), id: 'a' };
    const b = { ...newSourceFromPreset(deepseek, []), id: 'b' };
    return {
        ...cfg,
        sources: [a, b],
        models: [newOpenAiModel('a', 'm1'), newOpenAiModel('b', 'x'), newOpenAiModel('a', 'm2')],
        ai: { ...cfg.ai, default_provider_id: 'a/m2' },
    };
}

describe('providerDraft', () => {
    it('create draft gets an id that does not collide', () => {
        const cfg = { ...astrbotDefaultConfig(6199), sources: [newSourceFromPreset(deepseek, [])] };
        const d = draftForCreate(cfg, deepseek);
        expect(d.index).toBeNull();
        expect(d.src.id).toBe('deepseek-2');
        expect(d.kids).toEqual([]);
    });

    it('committing a new source appends it with its models and keeps the draft default', () => {
        const cfg = astrbotDefaultConfig(6199);
        const d = draftForCreate(cfg, deepseek);
        const kid = newOpenAiModel(d.src.id, 'deepseek-chat');
        const next = commitProviderDraft(cfg, { ...d, kids: [kid], defaultId: kid.id });
        expect(next.sources.map((s) => s.id)).toEqual(['deepseek']);
        expect(next.models.map((m) => m.id)).toEqual(['deepseek/deepseek-chat']);
        expect(next.ai.default_provider_id).toBe('deepseek/deepseek-chat');
    });

    it('editing keeps models in place and follows a renamed source', () => {
        const cfg = twoSources();
        const d = draftForEdit(cfg, 0);
        expect(d.kids.map((m) => m.model)).toEqual(['m1', 'm2']);
        const next = commitProviderDraft(cfg, { ...d, src: { ...d.src, id: ' a2 ' } });
        expect(next.sources.map((s) => s.id)).toEqual(['a2', 'b']);
        expect(next.models.map((m) => [m.model, m.provider_source_id])).toEqual([
            ['m1', 'a2'],
            ['m2', 'a2'],
            ['x', 'b'],
        ]);
    });

    it('drops the default when its model was removed in the draft', () => {
        const cfg = twoSources();
        const d = draftForEdit(cfg, 0);
        const next = commitProviderDraft(cfg, {
            ...d,
            kids: d.kids.filter((m) => m.model !== 'm2'),
        });
        expect(next.ai.default_provider_id).toBe('');
    });

    it('removing a source takes its models and a dangling default with it', () => {
        const cfg = twoSources();
        const next = removeProviderSource(cfg, 0);
        expect(next.sources.map((s) => s.id)).toEqual(['b']);
        expect(next.models.map((m) => m.id)).toEqual(['b/x']);
        expect(next.ai.default_provider_id).toBe('');
        const keep = removeProviderSource(cfg, 1);
        expect(keep.ai.default_provider_id).toBe('a/m2');
    });
});
