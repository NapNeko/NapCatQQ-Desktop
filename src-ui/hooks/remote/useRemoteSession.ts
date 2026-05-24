// 远端 SSH 主机 connect / 文件 / 运行时 / WebUI 的组合 hook。

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { remoteService } from '../../core/services/remote.service';
import type {
    ConnectRemoteHostRequest,
    RemoteHostConnectionInfo,
} from '../../core/ipc/types';

export function useRemoteSession(selectedBotId: string) {
    const queryClient = useQueryClient();
    const [connected, setConnected] = useState<RemoteHostConnectionInfo | null>(null);
    const [currentPath, setCurrentPath] = useState('/');

    const connectMutation = useMutation({
        mutationFn: (req: ConnectRemoteHostRequest) => remoteService.connect(req),
        onSuccess: (info) => {
            setConnected(info);
            queryClient.invalidateQueries({ queryKey: ['remoteFiles', info.remote_id, currentPath] });
            queryClient.invalidateQueries({ queryKey: ['remoteRuntime', info.remote_id, selectedBotId] });
            queryClient.invalidateQueries({ queryKey: ['remoteWebui', info.remote_id, selectedBotId] });
        },
    });

    const filesQuery = useQuery({
        queryKey: ['remoteFiles', connected?.remote_id, currentPath],
        queryFn: () => remoteService.listFiles(connected!.remote_id, currentPath),
        enabled: !!connected,
    });

    const runtimeQuery = useQuery({
        queryKey: ['remoteRuntime', connected?.remote_id, selectedBotId],
        queryFn: () => remoteService.runtimeStatus(connected!.remote_id, selectedBotId),
        enabled: !!connected && !!selectedBotId,
    });

    const webuiQuery = useQuery({
        queryKey: ['remoteWebui', connected?.remote_id, selectedBotId],
        queryFn: () => remoteService.webuiEndpoint(connected!.remote_id, selectedBotId),
        enabled: !!connected && !!selectedBotId,
    });

    return {
        connected,
        currentPath,
        setCurrentPath,
        connect: connectMutation.mutate,
        isConnecting: connectMutation.isPending,
        files: filesQuery.data ?? [],
        isFilesLoading: filesQuery.isLoading,
        runtimeStatus: runtimeQuery.data ?? null,
        webuiEndpoint: webuiQuery.data ?? null,
    };
}
