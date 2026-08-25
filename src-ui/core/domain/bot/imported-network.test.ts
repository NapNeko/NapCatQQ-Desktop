import { describe, expect, it } from 'vitest';
import type { ConnectConfig } from '../../ipc/generated/domain/ConnectConfig';
import { createDefaultBotConfig } from './config-defaults';
import { applyImportedNetwork } from './imported-network';

const emptyConnect: ConnectConfig = {
    httpServers: [],
    httpSseServers: [],
    httpClients: [],
    websocketServers: [],
    websocketClients: [],
    plugins: [],
};

describe('applyImportedNetwork', () => {
    it('fills connect, musicSignUrl and NC advanced fields', () => {
        const cfg = createDefaultBotConfig();
        applyImportedNetwork(cfg, {
            connect: {
                ...emptyConnect,
                httpServers: [
                    {
                        enable: true,
                        name: 'hs',
                        host: '0.0.0.0',
                        port: 3100,
                        path: '/',
                        token: 'nt',
                        debug: false,
                        enableCors: true,
                        enableWebsocket: false,
                        messagePostFormat: 'array',
                    },
                ],
            },
            musicSignUrl: 'https://nc.sign',
            enableLocalFile2Url: true,
            parseMultMsg: true,
        });
        expect(cfg.connect.httpServers).toHaveLength(1);
        expect(cfg.connect.httpServers[0].port).toBe(3100);
        expect(cfg.bot.musicSignUrl).toBe('https://nc.sign');
        expect(cfg.advanced.enableLocalFile2Url).toBe(true);
        expect(cfg.advanced.parseMultMsg).toBe(true);
        expect(cfg.statusCommand).toBeUndefined();
    });

    it('fills SL statusCommand without touching NC advanced defaults', () => {
        const cfg = createDefaultBotConfig();
        applyImportedNetwork(cfg, {
            connect: emptyConnect,
            statusCommand: { enabled: true, swallow: true, cooldownSeconds: 7 },
        });
        expect(cfg.statusCommand).toEqual({
            enabled: true,
            swallow: true,
            cooldownSeconds: 7,
        });
        expect(cfg.advanced.parseMultMsg).toBe(false);
        expect(cfg.advanced.enableLocalFile2Url).toBe(false);
        expect(cfg.bot.musicSignUrl).toBe('');
    });

    it('ignores empty musicSignUrl so desktop default stays', () => {
        const cfg = createDefaultBotConfig();
        cfg.bot.musicSignUrl = '';
        applyImportedNetwork(cfg, {
            connect: emptyConnect,
            musicSignUrl: '',
        });
        expect(cfg.bot.musicSignUrl).toBe('');
    });
});
