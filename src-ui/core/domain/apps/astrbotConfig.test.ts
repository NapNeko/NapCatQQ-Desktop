import { describe, expect, it } from 'vitest';
import { astrbotDefaultConfig, astrbotLinkInputsChanged, validateAstrBotConfig } from './astrbotConfig';

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
