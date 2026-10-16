// Windows 会话与 Agent 能力独立于 SSH 连接状态刷新。
import { useQuery } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

export const WINDOWS_REMOTE_INFO_KEY = 'remoteWindowsInfo';

export function windowsRemoteInfoQueryKey(server: ServerProfile) {
    return [
        WINDOWS_REMOTE_INFO_KEY,
        server.id,
        server.host,
        server.port,
        server.username,
        server.platform ?? 'auto',
    ] as const;
}

export function shouldProbeWindowsRemote(server: ServerProfile): boolean {
    return server.state === 'connected' && server.platform !== 'linux';
}

export function useWindowsRemoteInfo(server: ServerProfile) {
    return useQuery({
        queryKey: windowsRemoteInfoQueryKey(server),
        queryFn: () => serverService.windowsInfo(server.id),
        enabled: shouldProbeWindowsRemote(server),
        staleTime: 15_000,
        retry: false,
        refetchInterval: (query) =>
            server.state === 'connected' && query.state.data ? 30_000 : false,
    });
}
