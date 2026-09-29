import { describe, expect, it } from 'vitest';
import type { ConnectConfig } from '../../ipc/generated/domain/ConnectConfig';
import { createDefaultBotConfig } from './config-defaults';
import {
    applyImportedNetwork,
    isPreviewEmpty,
    previewImportedNetwork,
} from './imported-network';

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

const httpServer = (name: string, port: number) => ({
    enable: true,
    name,
    host: '127.0.0.1',
    port,
    path: '/',
    token: '',
    debug: false,
    enableCors: true,
    enableWebsocket: false,
    messagePostFormat: 'array' as const,
});

describe('previewImportedNetwork', () => {
    it('reports added / removed / changed connections by kind and name', () => {
        const cfg = createDefaultBotConfig();
        cfg.connect = {
            ...emptyConnect,
            httpServers: [httpServer('keep', 3000), httpServer('edit', 3001), httpServer('gone', 3002)],
        };
        const preview = previewImportedNetwork(cfg, {
            connect: {
                ...emptyConnect,
                httpServers: [httpServer('keep', 3000), httpServer('edit', 3999), httpServer('new', 3003)],
            },
        });
        expect(preview.added).toEqual([{ kind: 'httpServer', name: 'new' }]);
        expect(preview.removed).toEqual([{ kind: 'httpServer', name: 'gone' }]);
        expect(preview.changed).toEqual([{ kind: 'httpServer', name: 'edit' }]);
        expect(preview.otherChanged).toBe(false);
        expect(preview.next.connect.httpServers.map((c) => c.name)).toEqual(['keep', 'edit', 'new']);
    });

    it('does not mutate the current config', () => {
        const cfg = createDefaultBotConfig();
        const before = JSON.stringify(cfg);
        previewImportedNetwork(cfg, {
            connect: { ...emptyConnect, httpServers: [httpServer('new', 3003)] },
            parseMultMsg: !cfg.advanced.parseMultMsg,
        });
        expect(JSON.stringify(cfg)).toBe(before);
    });

    it('flags non-connection differences and treats key order as equal', () => {
        const cfg = createDefaultBotConfig();
        cfg.connect = { ...emptyConnect, httpServers: [httpServer('a', 3000)] };
        const reordered = Object.fromEntries(
            Object.entries(httpServer('a', 3000)).reverse(),
        ) as ReturnType<typeof httpServer>;
        const same = previewImportedNetwork(cfg, {
            connect: { ...emptyConnect, httpServers: [reordered] },
        });
        expect(isPreviewEmpty(same)).toBe(true);

        const other = previewImportedNetwork(cfg, {
            connect: { ...emptyConnect, httpServers: [httpServer('a', 3000)] },
            musicSignUrl: 'https://remote.sign',
        });
        expect(other.otherChanged).toBe(true);
        expect(isPreviewEmpty(other)).toBe(false);
    });
});
