// 浏览器预览模式下的 Remote SSH 假数据。
import type {
    ConnectRemoteHostRequest,
    RemoteFileEntry,
    RemoteHostConnectionInfo,
    RemoteRuntimeStatusResponse,
    RemoteWebuiEndpointResponse,
} from '../types';

const remoteConnections = new Map<string, RemoteHostConnectionInfo>();
const remoteFiles = new Map<string, RemoteFileEntry[]>([
    [
        '/',
        [
            { name: 'app', is_dir: true, size: 0 },
            { name: 'config', is_dir: true, size: 0 },
            { name: 'napcat_starter.sh', is_dir: false, size: 2048 },
            { name: 'README.md', is_dir: false, size: 450 },
        ],
    ],
    [
        '/config',
        [
            { name: 'onebot11.json', is_dir: false, size: 1024 },
            { name: 'quickstart.json', is_dir: false, size: 512 },
        ],
    ],
]);

export function mockConnectRemote(req: ConnectRemoteHostRequest): Promise<RemoteHostConnectionInfo> {
    return new Promise((resolve) => {
        setTimeout(() => {
            const info: RemoteHostConnectionInfo = {
                remote_id: req.remote_id,
                host: req.host,
                port: req.port || 22,
                username: req.username,
                webui_url: req.webui_url || `http://${req.host}:6099/webui`,
            };
            remoteConnections.set(req.remote_id, info);
            resolve(info);
        }, 800);
    });
}

export function mockListRemoteFiles(_remoteId: string, path: string): Promise<RemoteFileEntry[]> {
    return new Promise((resolve) => {
        setTimeout(() => {
            const normalized = path === '' || path === '/' ? '/' : path;
            const files = remoteFiles.get(normalized) || [
                { name: 'mock_file_1.log', is_dir: false, size: 4096 },
                { name: 'mock_file_2.conf', is_dir: false, size: 1024 },
            ];
            resolve(files);
        }, 400);
    });
}

export function mockRemoteRuntimeStatus(
    remoteId: string,
    botId: string,
): Promise<RemoteRuntimeStatusResponse> {
    return new Promise((resolve) => {
        setTimeout(() => {
            resolve({
                remote_id: remoteId,
                bot_id: botId,
                status: {
                    bot_id: botId,
                    state: 'running',
                    pid: 8848,
                    started_at: Math.floor((Date.now() - 3600000) / 1000),
                    memory_rss_bytes: 145000000,
                    server_total_memory_bytes: 8589934592,
                    backend_kind: 'remote_ssh',
                    runtime_target: 'remote_ssh',
                    extra: {
                        active_connections: 12,
                        webui_enabled: true,
                    },
                },
                backend_kind: 'remote_ssh',
                runtime_target: 'remote_ssh',
            });
        }, 500);
    });
}

export function mockRemoteWebuiEndpoint(
    remoteId: string,
    botId: string,
): Promise<RemoteWebuiEndpointResponse> {
    return new Promise((resolve) => {
        setTimeout(() => {
            const conn = remoteConnections.get(remoteId);
            resolve({
                remote_id: remoteId,
                bot_id: botId,
                webui_url: conn?.webui_url || 'http://127.0.0.1:6099/webui',
            });
        }, 300);
    });
}
