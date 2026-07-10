// 设置页：按远端 server 汇总 ncd-watch 安装态与该机 Bot 数。
// 多服务器各自独立；同步/安装按 server_id 调用，不混成全局开关。

import { useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';
import { isTauri } from '../../core/ipc/transport';
import { botService } from '../../core/services/bot.service';
import { componentService } from '../../core/services/component.service';
import { serverService } from '../../core/services/server.service';
import {
    isRuntimeTargetConcreteRemote,
    isRuntimeTargetLocal,
    normalizeRuntimeTargetFromDisk,
} from '../../core/domain/bot/runtime-target';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ServerState } from '../../core/ipc/generated/domain/ServerState';

export type NcdWatchServerRow = {
    serverId: string;
    name: string;
    hostLabel: string;
    state: ServerState;
    /** 落在该 server 上的 Bot 数量（配置侧 runtime_target） */
    botCount: number;
    /** detect 结果：true 已装 / false 未装 / null 探测中或失败 */
    watchInstalled: boolean | null;
    watchVersion: string | null;
    detectError: string | null;
};

function hostLabel(p: ServerProfile): string {
    const port = p.port && p.port !== 22 ? `:${p.port}` : '';
    return `${p.username}@${p.host}${port}`;
}

export function useNcdWatchServers(): {
    rows: NcdWatchServerRow[];
    loading: boolean;
    refetchAll: () => void;
} {
    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        enabled: isTauri,
        staleTime: 15_000,
    });

    const snapshotsQuery = useQuery({
        queryKey: ['botSnapshots'],
        queryFn: () => botService.listSnapshots(),
        enabled: isTauri,
        staleTime: 15_000,
    });

    const snapshots = snapshotsQuery.data ?? [];
    const configQueries = useQueries({
        queries: snapshots.map((s) => ({
            queryKey: ['botConfig', s.bot_id],
            queryFn: () => botService.getConfig(s.bot_id),
            enabled: isTauri && snapshots.length > 0,
            staleTime: 60_000,
        })),
    });

    const botCountByServer = useMemo(() => {
        const map = new Map<string, number>();
        configQueries.forEach((q) => {
            const cfg = q.data;
            if (!cfg?.bot) return;
            const t = cfg.bot.runtime_target;
            if (isRuntimeTargetLocal(t) || !isRuntimeTargetConcreteRemote(t)) return;
            const id = normalizeRuntimeTargetFromDisk(t);
            if (!id) return;
            map.set(id, (map.get(id) ?? 0) + 1);
        });
        return map;
    }, [configQueries]);

    const servers = serversQuery.data ?? [];
    const detectQueries = useQueries({
        queries: servers.map((p) => ({
            queryKey: ['component-detect', 'ncd_watch', `remote:${p.id}`],
            queryFn: () =>
                componentService.detectComponent('ncd_watch', `remote:${p.id}`),
            // 未连接也可尝试；失败时 UI 显示错误，不阻塞其它机
            enabled: isTauri && servers.length > 0,
            staleTime: 20_000,
            retry: false,
        })),
    });

    const rows: NcdWatchServerRow[] = useMemo(() => {
        return servers.map((p, i) => {
            const dq = detectQueries[i];
            let watchInstalled: boolean | null = null;
            let watchVersion: string | null = null;
            let detectError: string | null = null;
            if (dq?.isLoading || dq?.isFetching) {
                watchInstalled = null;
            } else if (dq?.isError) {
                watchInstalled = null;
                detectError =
                    dq.error instanceof Error
                        ? dq.error.message
                        : String(dq.error ?? '探测失败');
            } else if (dq?.data) {
                if (!dq.data.supported) {
                    watchInstalled = false;
                    detectError = '当前主机不支持 NCD Watch';
                } else if (dq.data.detected) {
                    watchInstalled = true;
                    watchVersion = dq.data.detected.version ?? null;
                } else {
                    watchInstalled = false;
                }
            }
            return {
                serverId: p.id,
                name: p.name?.trim() || p.host,
                hostLabel: hostLabel(p),
                state: p.state,
                botCount: botCountByServer.get(p.id) ?? 0,
                watchInstalled,
                watchVersion,
                detectError,
            };
        });
    }, [servers, detectQueries, botCountByServer]);

    const loading =
        serversQuery.isLoading ||
        (isTauri && servers.length > 0 && detectQueries.some((q) => q.isLoading));

    const refetchAll = () => {
        void serversQuery.refetch();
        void snapshotsQuery.refetch();
        detectQueries.forEach((q) => {
            void q.refetch();
        });
    };

    return { rows, loading, refetchAll };
}
