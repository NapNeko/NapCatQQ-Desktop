// 服务器状态条：看得见的时候每 3 秒读一次；读失败保留上一次的数，标一下「没读到」。

import { useQuery } from '@tanstack/react-query';
import { terminalService } from '../../core/services/terminal.service';
import type { ServerStats } from '../../core/ipc/generated/domain/ServerStats';

const INTERVAL_MS = 3000;

export function useTerminalStats(
    sessionId: string,
    enabled: boolean,
): { stats: ServerStats | null; stale: boolean } {
    const query = useQuery<ServerStats, Error>({
        queryKey: ['terminalStats', sessionId],
        queryFn: () => terminalService.stats(sessionId),
        enabled,
        // 旧实现失焦照轮；react-query 默认失焦暂停 refetchInterval，这里保持旧行为
        refetchIntervalInBackground: true,
        retry: false,
        staleTime: 0,
        refetchInterval: INTERVAL_MS,
    });
    // 失败时 react-query 留着上一次的 data，isError 对应旧实现的「最近一次没读到」
    return { stats: query.data ?? null, stale: query.isError };
}
