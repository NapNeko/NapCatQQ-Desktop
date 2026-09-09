// 远端某一路径的目录列表；query key 与旧 useRemoteSession 共用，避免重复打 SSH。

import { useQuery } from '@tanstack/react-query';
import { remoteService } from '../../core/services/remote.service';
import { normalizePosix } from '../../core/domain/remote-host/posixPath';
import type { RemoteFileEntry } from '../../core/ipc/types';

export const remoteFilesKey = (remoteId: string, path: string) =>
    ['remoteFiles', remoteId, normalizePosix(path)] as const;

export function useRemoteDirectory(remoteId: string | null, path: string, enabled = true) {
    const normalized = normalizePosix(path);
    return useQuery<RemoteFileEntry[], Error>({
        queryKey: remoteFilesKey(remoteId ?? '', normalized),
        queryFn: () => remoteService.listFiles(remoteId!, normalized),
        enabled: enabled && !!remoteId,
        staleTime: 10_000,
        retry: false,
    });
}
