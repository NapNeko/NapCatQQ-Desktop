import { describe, expect, it } from 'vitest';
import {
    isAppLinkConnectionName,
    replaceAppLinkClients,
} from './connections';
import type { WebsocketClientConfig } from '../../ipc/generated/domain/WebsocketClientConfig';

function client(name: string, url: string): WebsocketClientConfig {
    return {
        url,
        reportSelfMessage: false,
        heartInterval: 30000,
        reconnectInterval: 30000,
        role: 'Universal',
        enable: true,
        name,
        messagePostFormat: 'array',
        token: 't',
        debug: false,
    };
}

describe('replaceAppLinkClients', () => {
    it('keeps user connections and takes ncd-app slots from server', () => {
        const draft = [
            client('mine', 'ws://a'),
            client('ncd-app:k1', 'ws://old'),
        ];
        const server = [client('ncd-app:k1', 'ws://new')];
        expect(replaceAppLinkClients(draft, server)).toEqual([
            client('mine', 'ws://a'),
            client('ncd-app:k1', 'ws://new'),
        ]);
    });

    it('drops app-link clients after unlink', () => {
        const draft = [client('mine', 'ws://a'), client('ncd-app:k1', 'ws://old')];
        expect(replaceAppLinkClients(draft, [])).toEqual([client('mine', 'ws://a')]);
    });

    it('adds the server link when the dirty draft never had it', () => {
        const draft = [client('mine', 'ws://a')];
        const server = [client('ncd-app:k1', 'ws://new')];
        expect(replaceAppLinkClients(draft, server)).toEqual([
            client('mine', 'ws://a'),
            client('ncd-app:k1', 'ws://new'),
        ]);
    });

    it('recognizes the desktop prefix', () => {
        expect(isAppLinkConnectionName('ncd-app:k1')).toBe(true);
        expect(isAppLinkConnectionName('my-ws')).toBe(false);
    });
});
