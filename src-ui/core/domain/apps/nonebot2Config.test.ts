import { describe, expect, it } from 'vitest';
import {
    appendNoneBot2EnvPreset,
    nonebot2DefaultConfig,
    validateNoneBot2Config,
    NONEBOT2_ENV_PRESETS,
} from './nonebot2Config';

describe('NoneBot2 upstream configuration', () => {
    it('uses core defaults without enabling bare commands or extra separators', () => {
        const cfg = nonebot2DefaultConfig(8080);
        expect(cfg.env_prod.command_start).toEqual(['/']);
        expect(cfg.env_prod.command_sep).toEqual(['.']);
        expect(cfg.env_prod.api_timeout).toBe('30.0');
        expect(cfg.env_prod.session_expire_timeout).toBe('120');
        expect(validateNoneBot2Config(cfg)).toEqual([]);
    });

    it('validates timeout and HTTP / WebSocket settings with field paths', () => {
        const cfg = nonebot2DefaultConfig(8080);
        cfg.env_prod.api_timeout = 'Infinity';
        cfg.env_prod.onebot_ws_urls = ['https://example.com'];
        cfg.env_prod.onebot_api_roots = '{"10001":42}';
        expect(validateNoneBot2Config(cfg).map((issue) => issue.path)).toEqual([
            'env_prod/api_timeout',
            'env_prod/onebot_ws_urls/0',
            'env_prod/onebot_api_roots',
        ]);
        cfg.env_prod.api_timeout = '0.125';
        cfg.env_prod.session_expire_timeout = 'P1DT2H';
        cfg.env_prod.onebot_ws_urls = ['wss://example.com/ws'];
        cfg.env_prod.onebot_api_roots = '{"10001":"https://example.com/api"}';
        expect(validateNoneBot2Config(cfg)).toEqual([]);
    });

    it('adds driver settings on demand and preserves existing case-insensitive keys', () => {
        const existing = [{ key: 'fastapi_extra', value: '{"title":"custom"}', comment: 'keep' }];
        expect(appendNoneBot2EnvPreset(existing, 'FASTAPI_EXTRA')).toBe(existing);
        const next = appendNoneBot2EnvPreset(existing, 'QUART_RELOAD_DELAY');
        expect(next[0]).toBe(existing[0]);
        expect(next[1]).toEqual({
            key: 'QUART_RELOAD_DELAY',
            value: '0.25',
            comment: 'Quart 重载延迟（秒）',
        });
        expect(new Set(NONEBOT2_ENV_PRESETS.map((entry) => entry.key)).size).toBe(20);
    });

    it('rejects duplicate and reserved custom keys without rejecting plugin namespaces', () => {
        const cfg = nonebot2DefaultConfig(8080);
        cfg.env_prod.custom = [
            { key: 'PLUGIN__TOKEN', value: 'abc', comment: '' },
            { key: 'plugin__token', value: 'def', comment: '' },
            { key: 'ONEBOT_V11_SECRET', value: 'x', comment: '' },
        ];
        expect(validateNoneBot2Config(cfg).map((issue) => issue.path)).toEqual([
            'env_prod/custom/1/key',
            'env_prod/custom/2/key',
        ]);
    });
});
