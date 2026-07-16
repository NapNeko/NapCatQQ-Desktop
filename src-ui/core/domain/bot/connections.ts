// 连接配置纯函数：kind 元数据、新建默认值、查找 group 名、摘要文本。
//
// 所有规则集中在这里，UI 层只 dispatch 不做条件判断，避免 ConnectionsTab /
// ConnectionEditor 各写一份分支。

import type { ConnectConfig } from '../../ipc/generated/domain/ConnectConfig';
import type { HttpServerConfig } from '../../ipc/generated/domain/HttpServerConfig';
import type { HttpSseServerConfig } from '../../ipc/generated/domain/HttpSseServerConfig';
import type { HttpClientConfig } from '../../ipc/generated/domain/HttpClientConfig';
import type { WebsocketServerConfig } from '../../ipc/generated/domain/WebsocketServerConfig';
import type { WebsocketClientConfig } from '../../ipc/generated/domain/WebsocketClientConfig';
import type { BackendType } from '../../ipc/generated/domain/BackendType';

export type ConnectionKind =
    | 'httpServer'
    | 'httpSseServer'
    | 'httpClient'
    | 'websocketServer'
    | 'websocketClient';

export type ConnectionConfig =
    | HttpServerConfig
    | HttpSseServerConfig
    | HttpClientConfig
    | WebsocketServerConfig
    | WebsocketClientConfig;

/// kind → ConnectConfig 数组字段名。
export const CONNECTION_GROUP_KEY: Record<ConnectionKind, keyof ConnectConfig> = {
    httpServer: 'httpServers',
    httpSseServer: 'httpSseServers',
    httpClient: 'httpClients',
    websocketServer: 'websocketServers',
    websocketClient: 'websocketClients',
};

export interface ConnectionKindMeta {
    kind: ConnectionKind;
    /** 类型短标题，列表行 + chip 按钮共用。 */
    title: string;
    /** 类型长描述，新建按钮 hover tooltip / 选择器卡片副标题。 */
    description: string;
    /** SnowLuma 是否支持。SSE 仅 NapCat 有。 */
    supportedBackends: BackendType[];
    /** 默认实例名前缀，自动追加 random 3 位数字。 */
    namePrefix: string;
}

export const CONNECTION_KINDS: ReadonlyArray<ConnectionKindMeta> = [
    {
        kind: 'httpServer',
        title: 'HTTP 服务器',
        description: '开本地端口接收外部 API 调用',
        supportedBackends: ['napcat', 'snowluma'],
        namePrefix: 'http-server',
    },
    {
        kind: 'httpSseServer',
        title: 'HTTP SSE 服务器',
        description: '单向流式推送事件（仅 NapCat）',
        supportedBackends: ['napcat'],
        namePrefix: 'sse-server',
    },
    {
        kind: 'httpClient',
        title: 'HTTP Webhook',
        description: '主动 POST 投递事件到远端',
        supportedBackends: ['napcat', 'snowluma'],
        namePrefix: 'http-client',
    },
    {
        kind: 'websocketServer',
        // NapCat 文档：WebSocket 服务端 = 正向 WS（本端监听，外部连入）
        title: 'WS 正向服务器',
        description: '正向 WS：本端作服务端监听，外部连入双向通信',
        supportedBackends: ['napcat', 'snowluma'],
        namePrefix: 'ws-server',
    },
    {
        kind: 'websocketClient',
        // NapCat 文档：WebSocket 客户端 = 反向 WS（本端主动连远端）
        title: 'WS 反向客户端',
        description: '反向 WS：本端作客户端主动连远端，全双工低延迟',
        supportedBackends: ['napcat', 'snowluma'],
        namePrefix: 'ws-client',
    },
];

const META_BY_KIND: Record<ConnectionKind, ConnectionKindMeta> = Object.fromEntries(
    CONNECTION_KINDS.map((m) => [m.kind, m]),
) as Record<ConnectionKind, ConnectionKindMeta>;

export function getKindMeta(kind: ConnectionKind): ConnectionKindMeta {
    return META_BY_KIND[kind];
}

function randName(prefix: string): string {
    return `${prefix}-${Math.floor(Math.random() * 900 + 100)}`;
}

/// 生成 32 字节 base64url 随机 token，对齐 SnowLuma daemon 默认行为
/// (`randomBytes(32).toString('base64url')`)。NapCat 也接受这个格式。
function generateToken(): string {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    // base64url: A-Z a-z 0-9 - _，无 padding
    let b64 = btoa(String.fromCharCode(...bytes));
    b64 = b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    return b64;
}

export function createDefaultConnection(kind: ConnectionKind): ConnectionConfig {
    const namePrefix = META_BY_KIND[kind].namePrefix;
    const baseFields = {
        enable: true,
        name: randName(namePrefix),
        messagePostFormat: 'array' as const,
        token: generateToken(),
        debug: false,
    };
    switch (kind) {
        case 'httpServer':
            return {
                ...baseFields,
                host: '0.0.0.0',
                port: 3000,
                enableCors: true,
                enableWebsocket: false,
                path: '/',
            };
        case 'httpSseServer':
            return {
                ...baseFields,
                host: '0.0.0.0',
                port: 3001,
                enableCors: true,
                enableWebsocket: false,
                reportSelfMessage: false,
            };
        case 'httpClient':
            return {
                ...baseFields,
                url: '',
                reportSelfMessage: false,
            };
        case 'websocketServer':
            return {
                ...baseFields,
                host: '0.0.0.0',
                port: 3001,
                reportSelfMessage: false,
                enableForcePushEvent: true,
                heartInterval: 30000,
                path: '/',
                role: 'Universal',
            };
        case 'websocketClient':
            return {
                ...baseFields,
                url: '',
                reportSelfMessage: false,
                heartInterval: 30000,
                reconnectInterval: 5000,
                role: 'Universal',
            };
    }
}

/// 列表行的副标题摘要文本，例如 "http://0.0.0.0:3000/ · WS 共享：开"。
/// 没把它做成 ReactNode 的原因：摘要文本经常进 input search / aria-label，
/// 留 string 后面更灵活。
export function summarizeConnection(kind: ConnectionKind, c: ConnectionConfig): string {
    switch (kind) {
        case 'httpServer': {
            const v = c as HttpServerConfig;
            return `http://${v.host}:${v.port}${v.path || '/'} · WS 共享：${v.enableWebsocket ? '开' : '关'}`;
        }
        case 'httpSseServer': {
            const v = c as HttpSseServerConfig;
            return `http://${v.host}:${v.port} · 自报消息：${v.reportSelfMessage ? '开' : '关'}`;
        }
        case 'httpClient': {
            const v = c as HttpClientConfig;
            const timeout = v.timeoutMs ? ` · 超时 ${v.timeoutMs}ms` : '';
            return `${v.url || '(未填 URL)'}${timeout}`;
        }
        case 'websocketServer': {
            const v = c as WebsocketServerConfig;
            return `ws://${v.host}:${v.port}${v.path || '/'} · ${v.role} · 心跳 ${v.heartInterval}ms`;
        }
        case 'websocketClient': {
            const v = c as WebsocketClientConfig;
            return `${v.url || '(未填 URL)'} · ${v.role} · 重连 ${v.reconnectInterval}ms`;
        }
    }
}

/// 收集 ConnectConfig 中所有连接名，用于重名校验。
export function collectAllNames(connect: ConnectConfig): string[] {
    return [
        ...connect.httpServers.map((c) => c.name),
        ...connect.httpSseServers.map((c) => c.name),
        ...connect.httpClients.map((c) => c.name),
        ...connect.websocketServers.map((c) => c.name),
        ...connect.websocketClients.map((c) => c.name),
    ];
}

export interface ConnectionValidationOk { ok: true; }
export interface ConnectionValidationFail { ok: false; reason: string; }
export type ConnectionValidationResult = ConnectionValidationOk | ConnectionValidationFail;

/// 单条连接保存前校验。挡住明显错误，详细规则交给后端。
/// existingNames 不含自己（外层调用方先过滤）。
export function validateConnection(
    kind: ConnectionKind,
    c: ConnectionConfig,
    existingNames: string[],
): ConnectionValidationResult {
    const name = c.name.trim();
    if (!name) return { ok: false, reason: '连接名称不能为空' };
    if (existingNames.includes(name)) {
        return { ok: false, reason: `连接名 "${name}" 已存在，请改名` };
    }

    if (kind === 'httpServer' || kind === 'httpSseServer' || kind === 'websocketServer') {
        const v = c as HttpServerConfig | HttpSseServerConfig | WebsocketServerConfig;
        if (!Number.isFinite(v.port) || v.port < 1 || v.port > 65535) {
            return { ok: false, reason: '监听端口必须在 1 - 65535 之间' };
        }
    }

    if (kind === 'httpClient') {
        const v = c as HttpClientConfig;
        const url = v.url.trim();
        if (!url) return { ok: false, reason: '上报 Webhook URL 不能为空' };
        if (!url.startsWith('http://') && !url.startsWith('https://')) {
            return { ok: false, reason: 'Webhook URL 必须以 http:// 或 https:// 开头' };
        }
    }

    if (kind === 'websocketClient') {
        const v = c as WebsocketClientConfig;
        const url = v.url.trim();
        if (!url) return { ok: false, reason: '服务端 WebSocket URL 不能为空' };
        if (!url.startsWith('ws://') && !url.startsWith('wss://')) {
            return { ok: false, reason: 'WebSocket URL 必须以 ws:// 或 wss:// 开头' };
        }
        if (!Number.isFinite(v.reconnectInterval) || v.reconnectInterval < 1000) {
            return { ok: false, reason: '重连间隔不能低于 1000 毫秒' };
        }
    }

    if (kind === 'websocketServer' || kind === 'websocketClient') {
        const v = c as WebsocketServerConfig | WebsocketClientConfig;
        if (!Number.isFinite(v.heartInterval) || v.heartInterval < 1000) {
            return { ok: false, reason: '心跳间隔不能低于 1000 毫秒' };
        }
    }

    return { ok: true };
}
