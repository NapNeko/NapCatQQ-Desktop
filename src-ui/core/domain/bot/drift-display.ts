// drift-display.ts: translate raw JSON drift entries into human-readable
// presentation objects for ConfigDriftDialog.
//
// Design: each field path has an adapter that knows the business semantics.
// Unknown paths fall back to a generic JSON display.

import type { DriftEntry } from '../../ipc/generated/DriftEntry';

// ─── Public types ────────────────────────────────────────────────────────────

export interface DriftDisplayEntry {
    key: string;
    /** Human label for the field, e.g. "HTTP 服务器列表" */
    label: string;
    /** File name (short) */
    file: string;
    /** Raw path for building DriftDecision */
    path: string;
    /** Left side (internal / ours) display */
    ours: DriftDisplayValue;
    /** Right side (external / file) display */
    theirs: DriftDisplayValue;
}

export type DriftDisplayValue =
    | { kind: 'scalar'; text: string }
    | { kind: 'connections'; items: ConnectionSummary[] }
    | { kind: 'json'; preview: string };

export interface ConnectionSummary {
    name: string;
    type: string;
    endpoint: string;
    enabled: boolean;
    token: string;
}

// ─── Adapter registry ────────────────────────────────────────────────────────

type Adapter = (entry: DriftEntry) => Omit<DriftDisplayEntry, 'key' | 'file' | 'path'>;

const ADAPTERS: Record<string, Adapter> = {
    'networks.httpServers': connectionsAdapter('HTTP 服务器', 'HTTP'),
    'networks.wsServers': connectionsAdapter('WebSocket 服务器', 'WS-Server'),
    'networks.httpClients': connectionsAdapter('HTTP Webhook', 'Webhook'),
    'networks.wsClients': connectionsAdapter('WebSocket 客户端', 'WS-Client'),
    // NapCat onebot11 uses "network" (singular) with sub-keys
    'network.httpServers': connectionsAdapter('HTTP 服务器', 'HTTP'),
    'network.httpSseServers': connectionsAdapter('HTTP SSE 服务器', 'SSE'),
    'network.httpClients': connectionsAdapter('HTTP Webhook', 'Webhook'),
    'network.websocketServers': connectionsAdapter('WebSocket 服务器', 'WS-Server'),
    'network.websocketClients': connectionsAdapter('WebSocket 客户端', 'WS-Client'),
    'musicSignUrl': scalarAdapter('音乐签名接口'),
    'enableLocalFile2Url': boolAdapter('本地文件转 URL'),
    'parseMultMsg': boolAdapter('合并消息解析'),
    'fileLog': boolAdapter('文件日志'),
    'consoleLog': boolAdapter('控制台日志'),
    'fileLogLevel': scalarAdapter('文件日志等级'),
    'consoleLogLevel': scalarAdapter('控制台日志等级'),
    'packetBackend': scalarAdapter('封包后端模式'),
    'packetServer': scalarAdapter('封包服务地址'),
    'o3HookMode': scalarAdapter('O3 Hook 模式'),
    'bypass': jsonAdapter('反检测开关'),
    'bypass.hook': boolAdapter('反检测: Hook'),
    'bypass.window': boolAdapter('反检测: Window'),
    'bypass.module': boolAdapter('反检测: Module'),
    'bypass.process': boolAdapter('反检测: Process'),
    'bypass.container': boolAdapter('反检测: Container'),
    'bypass.js': boolAdapter('反检测: JS'),
};

// ─── Main transform function ─────────────────────────────────────────────────

export function transformDriftEntries(entries: DriftEntry[]): DriftDisplayEntry[] {
    return entries.map((entry) => {
        const adapter = ADAPTERS[entry.path];
        const base = adapter ? adapter(entry) : fallbackAdapter(entry);
        return {
            key: `${entry.file}::${entry.path}`,
            file: entry.file,
            path: entry.path,
            ...base,
        };
    });
}


// ─── Adapter factories ───────────────────────────────────────────────────────

function scalarAdapter(label: string): Adapter {
    return (entry) => ({
        label,
        ours: { kind: 'scalar', text: formatScalar(entry.internal) },
        theirs: { kind: 'scalar', text: formatScalar(entry.external) },
    });
}

function boolAdapter(label: string): Adapter {
    return (entry) => ({
        label,
        ours: { kind: 'scalar', text: formatBool(entry.internal) },
        theirs: { kind: 'scalar', text: formatBool(entry.external) },
    });
}

function jsonAdapter(label: string): Adapter {
    return (entry) => ({
        label,
        ours: { kind: 'json', preview: formatJson(entry.internal) },
        theirs: { kind: 'json', preview: formatJson(entry.external) },
    });
}

function connectionsAdapter(label: string, type: string): Adapter {
    return (entry) => ({
        label,
        ours: { kind: 'connections', items: parseConnections(entry.internal, type) },
        theirs: { kind: 'connections', items: parseConnections(entry.external, type) },
    });
}

function fallbackAdapter(entry: DriftEntry): Omit<DriftDisplayEntry, 'key' | 'file' | 'path'> {
    const label = entry.path.split('.').pop() ?? entry.path;
    // If both are simple scalars, use scalar display
    if (isSimple(entry.internal) && isSimple(entry.external)) {
        return {
            label,
            ours: { kind: 'scalar', text: formatScalar(entry.internal) },
            theirs: { kind: 'scalar', text: formatScalar(entry.external) },
        };
    }
    return {
        label,
        ours: { kind: 'json', preview: formatJson(entry.internal) },
        theirs: { kind: 'json', preview: formatJson(entry.external) },
    };
}

// ─── Formatting helpers ──────────────────────────────────────────────────────

function isSimple(v: unknown): boolean {
    return v === null || v === undefined || typeof v !== 'object';
}

function formatScalar(v: unknown): string {
    if (v === null || v === undefined) return '(空)';
    if (typeof v === 'boolean') return v ? '开启' : '关闭';
    if (typeof v === 'number') return String(v);
    if (typeof v === 'string') return v || '(空字符串)';
    return JSON.stringify(v);
}

function formatBool(v: unknown): string {
    if (v === true || v === 1) return '开启';
    if (v === false || v === 0) return '关闭';
    return formatScalar(v);
}

function formatJson(v: unknown): string {
    if (v === null || v === undefined) return 'null';
    return JSON.stringify(v, null, 2);
}

function parseConnections(v: unknown, type: string): ConnectionSummary[] {
    if (!Array.isArray(v)) return [];
    return v.map((item: Record<string, unknown>) => {
        const name = String(item.name ?? item.Name ?? '(unnamed)');
        const enabled = Boolean(item.enable ?? item.enabled ?? true);
        const token = maskToken(String(item.token ?? item.accessToken ?? ''));

        let endpoint = '';
        if (item.host && item.port) {
            const path = item.path ?? '/';
            const proto = type.startsWith('WS') ? 'ws' : 'http';
            endpoint = `${proto}://${item.host}:${item.port}${path}`;
        } else if (item.url) {
            endpoint = String(item.url);
        }

        return { name, type, endpoint, enabled, token };
    });
}

function maskToken(token: string): string {
    if (!token) return '(无)';
    if (token.length <= 6) return '••••••';
    return token.slice(0, 3) + '••••' + token.slice(-3);
}
