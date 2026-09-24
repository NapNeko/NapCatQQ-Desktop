import { describe, expect, it } from 'vitest';
import {
    astrbotConfigWarnings,
    astrbotDefaultConfig,
    astrbotLinkInputsChanged,
    astrbotSetup,
    astrbotWakeHint,
    validateAstrBotConfig,
} from './astrbotConfig';
import type { AstrBotInstanceConfig, AstrBotProviderSource } from '../../ipc/types';

function source(id: string, providerType = 'chat_completion', enable = true): AstrBotProviderSource {
    return {
        id,
        provider: 'openai',
        type: 'openai_chat_completion',
        provider_type: providerType,
        enable,
        key: [],
        api_base: '',
        timeout: 120,
        proxy: '',
    };
}

function withChatModel(cfg: AstrBotInstanceConfig, sourceId = 'ds', model = 'deepseek-chat'): AstrBotInstanceConfig {
    return {
        ...cfg,
        sources: [...cfg.sources, source(sourceId)],
        models: [
            ...cfg.models,
            {
                id: `${sourceId}/${model}`,
                enable: true,
                provider_source_id: sourceId,
                model,
                modalities: ['text'],
                max_context_tokens: 0,
            },
        ],
    };
}

describe('astrbotSetup', () => {
    it('fresh instance needs a link and a chat source', () => {
        const s = astrbotSetup(astrbotDefaultConfig(6199), false);
        expect(s).toEqual({ linkDone: false, llmIssue: 'no_source', chatSourceIndex: -1, ready: false });
    });

    it('other platforms count as connected', () => {
        const cfg = { ...astrbotDefaultConfig(6199), other_platforms: ['telegram'] };
        expect(astrbotSetup(cfg, false).linkDone).toBe(true);
    });

    it('embedding-only source is not a chat source', () => {
        const cfg = { ...astrbotDefaultConfig(6199), sources: [source('emb', 'embedding')] };
        expect(astrbotSetup(cfg, true).llmIssue).toBe('no_source');
    });

    it('chat source without usable model points at that source', () => {
        const cfg = { ...astrbotDefaultConfig(6199), sources: [source('emb', 'embedding'), source('ds')] };
        const s = astrbotSetup(cfg, true);
        expect(s.llmIssue).toBe('no_model');
        expect(s.chatSourceIndex).toBe(1);
    });

    it('model under a disabled source is not usable', () => {
        const base = withChatModel(astrbotDefaultConfig(6199));
        const cfg = { ...base, sources: base.sources.map((x) => ({ ...x, enable: false })) };
        const s = astrbotSetup(cfg, true);
        expect(s.llmIssue).toBe('no_model');
        expect(s.chatSourceIndex).toBe(0);
    });

    it('usable model but no default', () => {
        expect(astrbotSetup(withChatModel(astrbotDefaultConfig(6199)), true).llmIssue).toBe('no_default');
    });

    it('llm switch off comes after the model checks', () => {
        const base = withChatModel(astrbotDefaultConfig(6199));
        const cfg = { ...base, ai: { ...base.ai, default_provider_id: 'ds/deepseek-chat', enable: false } };
        expect(astrbotSetup(cfg, true).llmIssue).toBe('llm_off');
    });

    it('ready when linked and a default chat model is on', () => {
        const base = withChatModel(astrbotDefaultConfig(6199));
        const cfg = { ...base, ai: { ...base.ai, default_provider_id: 'ds/deepseek-chat' } };
        expect(astrbotSetup(cfg, true)).toEqual({ linkDone: true, llmIssue: null, chatSourceIndex: 0, ready: true });
        expect(astrbotSetup(cfg, false).ready).toBe(false);
    });
});

describe('astrbotWakeHint', () => {
    const withGates = (patch: Partial<AstrBotInstanceConfig['gates']>, llmPrefix = '') => {
        const cfg = astrbotDefaultConfig(6199);
        return { ...cfg, gates: { ...cfg.gates, ...patch }, ai: { ...cfg.ai, wake_prefix: llmPrefix } };
    };

    it('default: @ or slash in groups, no prefix in private chat', () => {
        expect(astrbotWakeHint(astrbotDefaultConfig(6199))).toEqual({
            sentence: '群里 @它 或用「/」开头发消息，私聊直接发',
            short: '@它 或 / 开头',
        });
    });

    it('no prefix means @ only', () => {
        expect(astrbotWakeHint(withGates({ wake_prefix: [] }))).toEqual({
            sentence: '群里 @它 发消息，私聊直接发',
            short: '@它',
        });
    });

    it('lists every prefix', () => {
        const h = astrbotWakeHint(withGates({ wake_prefix: ['/', '#'] }));
        expect(h.sentence).toBe('群里 @它 或用「/」或「#」开头发消息，私聊直接发');
        expect(h.short).toBe('@它 或 / # 开头');
    });

    it('private chat that needs a prefix', () => {
        expect(astrbotWakeHint(withGates({ friend_message_needs_wake_prefix: true })).sentence).toBe(
            '群里 @它 或用「/」开头发消息，私聊也要带前缀',
        );
        expect(
            astrbotWakeHint(withGates({ wake_prefix: [], friend_message_needs_wake_prefix: true })).sentence,
        ).toBe('群里 @它 发消息，私聊没法唤醒');
    });

    it('an empty prefix wakes on every message', () => {
        const h = astrbotWakeHint(withGates({ wake_prefix: ['', '/'], friend_message_needs_wake_prefix: true }));
        expect(h.sentence).toBe('群里每条消息都会叫醒它，私聊直接发');
        expect(h.short).toBe('每条消息');
    });

    it('mentions the extra LLM prefix', () => {
        expect(astrbotWakeHint(withGates({}, 'ai')).sentence).toBe(
            '群里 @它 或用「/」开头发消息，私聊直接发；走大模型还要再带「ai」',
        );
    });
});

describe('astrbotConfigWarnings', () => {
    it('defaults have nothing to warn about', () => {
        expect(astrbotConfigWarnings(astrbotDefaultConfig(6199))).toEqual([]);
    });

    it('speech switches without a provider', () => {
        const cfg = astrbotDefaultConfig(6199);
        const w = astrbotConfigWarnings({
            ...cfg,
            stt: { ...cfg.stt, enable: true },
            tts: { ...cfg.tts, enable: true, provider_id: '' },
        });
        expect(w.map((x) => [x.key, x.area])).toEqual([
            ['stt', 'talk'],
            ['tts', 'talk'],
        ]);
    });

    it('subagent switch on with nothing or half-filled agents', () => {
        const cfg = astrbotDefaultConfig(6199);
        const on = { ...cfg, subagent: { ...cfg.subagent, main_enable: true } };
        expect(astrbotConfigWarnings(on).map((x) => x.key)).toEqual(['subagent-empty']);
        const half = { ...on, subagent: { ...on.subagent, agents: [{ provider_id: 'ds/x', persona_id: '' }] } };
        const w = astrbotConfigWarnings(half);
        expect(w.map((x) => [x.key, x.area])).toEqual([['subagent-incomplete', 'subagent']]);
        expect(w[0].text).toContain('1 个');
    });
});

describe('astrbotConfig', () => {
    it('defaults OneBot to instance port and WebUI to 6185', () => {
        const cfg = astrbotDefaultConfig(6199);
        expect(cfg.onebot.ws_reverse_port).toBe(6199);
        expect(cfg.onebot.ws_reverse_host).toBe('0.0.0.0');
        expect(cfg.dashboard_port).toBe(6185);
        expect(cfg.claimed).toBe(false);
        expect(cfg.sources).toEqual([]);
        expect(cfg.ai.enable).toBe(true);
        expect(cfg.gates.enable_id_white_list).toBe(true);
    });

    it('rejects duplicate source id and orphan model', () => {
        const dup = astrbotDefaultConfig(6199);
        const src = {
            id: 'a',
            provider: 'openai',
            type: 'openai_chat_completion',
            provider_type: 'chat_completion',
            enable: true,
            key: [] as string[],
            api_base: '',
            timeout: 120,
            proxy: '',
        };
        dup.sources = [src, { ...src }];
        expect(validateAstrBotConfig(dup).some((i) => i.path === 'sources/1/id')).toBe(true);
        const orphan = astrbotDefaultConfig(6199);
        orphan.sources = [src];
        orphan.models = [
            {
                id: 'a/m',
                enable: true,
                provider_source_id: 'missing',
                model: 'm',
                modalities: ['text'],
                max_context_tokens: 0,
            },
        ];
        expect(validateAstrBotConfig(orphan).some((i) => i.path === 'models/0/provider_source_id')).toBe(true);
    });

    it('rejects OneBot port colliding with WebUI', () => {
        const cfg = astrbotDefaultConfig(6185);
        cfg.dashboard_port = 6185;
        const issues = validateAstrBotConfig(cfg);
        expect(issues.some((i) => i.path === 'onebot/ws_reverse_port')).toBe(true);
    });

    it('link inputs watch port and token only', () => {
        const a = astrbotDefaultConfig(6199);
        const b = { ...a, onebot: { ...a.onebot, ws_reverse_host: '127.0.0.1' } };
        expect(astrbotLinkInputsChanged(a, b)).toBe(false);
        expect(
            astrbotLinkInputsChanged(a, { ...a, onebot: { ...a.onebot, ws_reverse_token: 'x' } }),
        ).toBe(true);
    });
});
