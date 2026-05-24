// 远端 SSH 主机 / 文件 / 运行时 / WebUI 端点 IPC 服务。

import { invoke, isTauri } from '../ipc/transport';
import type {
    ConnectRemoteHostRequest,
    RemoteFileEntry,
    RemoteHostConnectionInfo,
    RemoteRuntimeStatusResponse,
    RemoteWebuiEndpointResponse,
} from '../ipc/types';
import {
    mockConnectRemote,
    mockListRemoteFiles,
    mockRemoteRuntimeStatus,
    mockRemoteWebuiEndpoint,
} from '../ipc/mock/remote.mock';

export const remoteService = {
    connect: async (request: ConnectRemoteHostRequest): Promise<RemoteHostConnectionInfo> => {
        if (isTauri) return invoke<RemoteHostConnectionInfo>('connect_remote_host', { request });
        return mockConnectRemote(request);
    },

    listFiles: async (remoteId: string, path: string): Promise<RemoteFileEntry[]> => {
        if (isTauri) {
            return invoke<RemoteFileEntry[]>('list_remote_files', {
                request: { remote_id: remoteId, path },
            });
        }
        return mockListRemoteFiles(remoteId, path);
    },

    runtimeStatus: async (
        remoteId: string,
        botId: string,
    ): Promise<RemoteRuntimeStatusResponse> => {
        if (isTauri) {
            return invoke<RemoteRuntimeStatusResponse>('get_remote_runtime_status', {
                request: { remote_id: remoteId, bot_id: botId },
            });
        }
        return mockRemoteRuntimeStatus(remoteId, botId);
    },

    webuiEndpoint: async (
        remoteId: string,
        botId: string,
    ): Promise<RemoteWebuiEndpointResponse> => {
        if (isTauri) {
            return invoke<RemoteWebuiEndpointResponse>('get_remote_webui_endpoint', {
                request: { remote_id: remoteId, bot_id: botId },
            });
        }
        return mockRemoteWebuiEndpoint(remoteId, botId);
    },
};
