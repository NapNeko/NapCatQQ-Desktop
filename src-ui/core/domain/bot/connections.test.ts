import { describe, expect, it } from 'vitest';
import type { ConnectConfig } from '../../ipc/generated/domain/ConnectConfig';
import type { HttpClientConfig } from '../../ipc/generated/domain/HttpClientConfig';
import type { HttpServerConfig } from '../../ipc/generated/domain/HttpServerConfig';
import type { HttpSseServerConfig } from '../../ipc/generated/domain/HttpSseServerConfig';
import type { WebsocketClientConfig } from '../../ipc/generated/domain/WebsocketClientConfig';
import type { WebsocketServerConfig } from '../../ipc/generated/domain/WebsocketServerConfig';
import {
    APP_LINK_CONNECTION_PREFIX,
    CONNECTION_GROUP_KEY,
    CONNECTION_KINDS,
    collectAllNames,
    createDefaultConnection,
    getKindMeta,
    isAppLinkConnectionName,
    replaceAppLinkClients,
    summarizeConnection,
    validateConnection,
    type ConnectionKind,
} from './connections';

const httpServer = (over: Partial<HttpServerConfig> = {}): HttpServerConfig => ({
    host: '0.0.0.0',
    port: 3000,
    enableCors: true,
    enableWebsocket: false,
    path: '/',
    enable: true,
    name: 'http-server-100',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    ...over,
});
const sseServer = (over: Partial<HttpSseServerConfig> = {}): HttpSseServerConfig => ({
    host: '0.0.0.0',
    port: 3001,
    enableCors: true,
    enableWebsocket: false,
    reportSelfMessage: false,
    enable: true,
    name: 'sse-server-100',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    ...over,
});
const httpClient = (over: Partial<HttpClientConfig> = {}): HttpClientConfig => ({
    url: '',
    reportSelfMessage: false,
    enable: true,
    name: 'http-client-100',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    ...over,
});
const wsServer = (over: Partial<WebsocketServerConfig> = {}): WebsocketServerConfig => ({
    host: '0.0.0.0',
    port: 3002,
    reportSelfMessage: false,
    enableForcePushEvent: true,
    heartInterval: 30000,
    path: '/',
    role: 'Universal',
    enable: true,
    name: 'ws-server-100',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    ...over,
});
const wsClient = (over: Partial<WebsocketClientConfig> = {}): WebsocketClientConfig => ({
    url: '',
    reportSelfMessage: false,
    heartInterval: 30000,
    reconnectInterval: 5000,
    role: 'Universal',
    enable: true,
    name: 'ws-client-100',
    messagePostFormat: 'array',
    token: '',
    debug: false,
    ...over,
});

describe('kind 元数据', () => {
    it('五个 kind 全有 meta，且各自映射到 ConnectConfig 的数组字段', () => {
        const kinds: ConnectionKind[] = [
            'httpServer',
            'httpSseServer',
            'httpClient',
            'websocketServer',
            'websocketClient',
        ];
        expect(CONNECTION_KINDS.map((m) => m.kind)).toEqual(kinds);
        for (const kind of kinds) {
            expect(getKindMeta(kind).kind).toBe(kind);
            expect(CONNECTION_GROUP_KEY[kind]).toBeTruthy();
        }
        expect(CONNECTION_GROUP_KEY.httpSseServer).toBe('httpSseServers');
        expect(CONNECTION_GROUP_KEY.websocketClient).toBe('websocketClients');
    });

    it('SSE 只有 NapCat 支持，其余两个后端都行', () => {
        expect(getKindMeta('httpSseServer').supportedBackends).toEqual(['napcat']);
        expect(getKindMeta('httpServer').supportedBackends).toEqual(['napcat', 'snowluma']);
    });
});

describe('createDefaultConnection', () => {
    it('每个 kind 给出启用态、随机名和随机 token', () => {
        for (const meta of CONNECTION_KINDS) {
            const c = createDefaultConnection(meta.kind);
            expect(c.enable).toBe(true);
            expect(c.name).toMatch(new RegExp(`^${meta.namePrefix}-\\d{3}$`));
            expect(c.messagePostFormat).toBe('array');
            expect(c.debug).toBe(false);
            // 32 字节 base64url：43 字符、无 padding
            expect(c.token).toMatch(/^[A-Za-z0-9_-]{43}$/);
        }
    });

    it('两次生成的名字与 token 不重样', () => {
        const a = createDefaultConnection('websocketClient');
        const b = createDefaultConnection('websocketClient');
        expect(a.token).not.toBe(b.token);
        // 名字前缀相同、随机段几乎必然不同；同名时 token 仍必须不同
        expect(a.name.startsWith('ws-client-')).toBe(true);
        expect(b.name.startsWith('ws-client-')).toBe(true);
    });

    it('监听型走端口，外连型走空 URL 待用户填', () => {
        const hs = createDefaultConnection('httpServer');
        expect([hs.host, hs.port, hs.path]).toEqual(['0.0.0.0', 3000, '/']);
        const wss = createDefaultConnection('websocketServer');
        expect([wss.port, wss.role, wss.heartInterval]).toEqual([3001, 'Universal', 30000]);
        expect(createDefaultConnection('httpClient').url).toBe('');
        const wsc = createDefaultConnection('websocketClient');
        expect([wsc.url, wsc.reconnectInterval, wsc.role]).toEqual(['', 5000, 'Universal']);
    });
});

describe('summarizeConnection', () => {
    it('每种 kind 拼出各自的副标题', () => {
        expect(summarizeConnection('httpServer', httpServer())).toBe(
            'http://0.0.0.0:3000/ · WS 共享：关',
        );
        expect(
            summarizeConnection('httpServer', httpServer({ enableWebsocket: true, path: '' })),
        ).toBe('http://0.0.0.0:3000/ · WS 共享：开');
        expect(summarizeConnection('httpSseServer', sseServer({ reportSelfMessage: true }))).toBe(
            'http://0.0.0.0:3001 · 自报消息：开',
        );
        expect(
            summarizeConnection(
                'httpClient',
                httpClient({ url: 'https://hook.example.com/api', timeoutMs: 5000 }),
            ),
        ).toBe('https://hook.example.com/api · 超时 5000ms');
        expect(summarizeConnection('httpClient', httpClient())).toBe('(未填 URL)');
        expect(summarizeConnection('websocketServer', wsServer({ role: 'Api' }))).toBe(
            'ws://0.0.0.0:3002/ · Api · 心跳 30000ms',
        );
        expect(
            summarizeConnection(
                'websocketClient',
                wsClient({ url: 'wss://remote.example.com/ws' }),
            ),
        ).toBe('wss://remote.example.com/ws · Universal · 重连 5000ms');
        expect(summarizeConnection('websocketClient', wsClient())).toBe(
            '(未填 URL) · Universal · 重连 5000ms',
        );
    });
});

describe('collectAllNames', () => {
    it('五个分组的名字全部收齐', () => {
        const connect: ConnectConfig = {
            httpServers: [httpServer({ name: 'a' })],
            httpSseServers: [sseServer({ name: 'b' })],
            httpClients: [httpClient({ name: 'c' })],
            websocketServers: [wsServer({ name: 'd' })],
            websocketClients: [wsClient({ name: 'e' })],
            plugins: [],
        };
        expect(collectAllNames(connect)).toEqual(['a', 'b', 'c', 'd', 'e']);
        expect(
            collectAllNames({
                httpServers: [],
                httpSseServers: [],
                httpClients: [],
                websocketServers: [],
                websocketClients: [],
                plugins: [],
            }),
        ).toEqual([]);
    });
});

describe('validateConnection', () => {
    it('名字空 / 重名先挡', () => {
        expect(validateConnection('httpServer', httpServer({ name: '  ' }), []).ok).toBe(false);
        const dup = validateConnection('httpServer', httpServer({ name: 'a' }), ['a']);
        expect(dup.ok).toBe(false);
        expect(validateConnection('httpServer', httpServer(), []).ok).toBe(true);
    });

    it('监听端口 1-65535，外连型不查端口', () => {
        for (const port of [0, 65536, Number.NaN]) {
            expect(validateConnection('httpServer', httpServer({ port }), []).ok).toBe(false);
            expect(validateConnection('websocketServer', wsServer({ port }), []).ok).toBe(false);
        }
        expect(validateConnection('httpServer', httpServer({ port: 1 }), []).ok).toBe(true);
        expect(validateConnection('httpServer', httpServer({ port: 65535 }), []).ok).toBe(true);
        expect(validateConnection('httpClient', httpClient({ url: 'https://x' }), []).ok).toBe(
            true,
        );
    });

    it('Webhook 要 http(s) URL，反向 WS 要 ws(s) URL', () => {
        expect(validateConnection('httpClient', httpClient(), []).ok).toBe(false);
        expect(validateConnection('httpClient', httpClient({ url: 'ftp://x' }), []).ok).toBe(false);
        expect(
            validateConnection('httpClient', httpClient({ url: ' https://hook.example.com ' }), [])
                .ok,
        ).toBe(true);
        expect(validateConnection('websocketClient', wsClient(), []).ok).toBe(false);
        expect(
            validateConnection('websocketClient', wsClient({ url: 'http://x:8080' }), []).ok,
        ).toBe(false);
        expect(
            validateConnection(
                'websocketClient',
                wsClient({ url: 'wss://remote.example.com/ws' }),
                [],
            ).ok,
        ).toBe(true);
    });

    it('重连与心跳间隔各守 1000ms 下限', () => {
        expect(
            validateConnection(
                'websocketClient',
                wsClient({ url: 'ws://x', reconnectInterval: 999 }),
                [],
            ).ok,
        ).toBe(false);
        expect(
            validateConnection(
                'websocketClient',
                wsClient({ url: 'ws://x', heartInterval: 500 }),
                [],
            ).ok,
        ).toBe(false);
        expect(validateConnection('websocketServer', wsServer({ heartInterval: 500 }), []).ok).toBe(
            false,
        );
        expect(
            validateConnection(
                'websocketClient',
                wsClient({ url: 'ws://x', reconnectInterval: 1000, heartInterval: 1000 }),
                [],
            ).ok,
        ).toBe(true);
    });
});

describe('对接连接隔离', () => {
    it('ncd-app: 前缀识别', () => {
        expect(isAppLinkConnectionName(`${APP_LINK_CONNECTION_PREFIX}onebot`)).toBe(true);
        expect(isAppLinkConnectionName('ws-client-101')).toBe(false);
        expect(isAppLinkConnectionName('')).toBe(false);
    });

    it('用户手改保留、对接连接整组换成服务端版本', () => {
        const draft = [
            wsClient({ name: 'ws-client-101' }),
            wsClient({ name: 'ncd-app:onebot', url: 'ws://脏数据' }),
        ];
        const server = [
            wsClient({ name: 'ncd-app:onebot', url: 'ws://127.0.0.1:7777' }),
            wsClient({ name: 'ws-server-noise' }),
        ];
        const merged = replaceAppLinkClients(draft, server);
        expect(merged.map((c) => c.name)).toEqual(['ws-client-101', 'ncd-app:onebot']);
        expect(merged[1].url).toBe('ws://127.0.0.1:7777');
    });

    it('两边都没有对接连接时原样返回用户连接', () => {
        const draft = [wsClient({ name: 'a' })];
        expect(replaceAppLinkClients(draft, [wsClient({ name: 'b' })])).toEqual(draft);
        expect(replaceAppLinkClients([], [])).toEqual([]);
    });
});
