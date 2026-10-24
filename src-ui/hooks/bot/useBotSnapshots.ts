// 实时同步 Bot snapshot 列表的 hook。
// useQuery + bot_state_changed 事件合并 → 单一 source of truth。

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { botService } from '../../core/services/bot.service';
import { useDomainEvents } from '../events/useDomainEvents';
import { perfMark } from '../../core/domain/performance/perfMarks';
import type { BotActorSnapshot } from '../../core/ipc/types';
import { mergeBotSnapshotList, updateBotSnapshots } from './botSnapshotCache';

const BOT_SNAPSHOTS_KEY = ['botSnapshots'] as const;

export function useBotSnapshots(options?: { disablePolling?: boolean }) {
    const queryClient = useQueryClient();
    const mountedAt = useRef(Date.now());
    const firstDataMarked = useRef(false);

    const query = useQuery<BotActorSnapshot[], Error>({
        queryKey: BOT_SNAPSHOTS_KEY,
        queryFn: async () => {
            const snapshots = await botService.listSnapshots();
            return mergeBotSnapshotList(
                queryClient.getQueryData<BotActorSnapshot[]>(BOT_SNAPSHOTS_KEY),
                snapshots,
            );
        },
        // 补读的是本地 Actor 快照；事件丢失后也能恢复卡片，不重复探测 SSH。
        refetchInterval: (query) =>
            options?.disablePolling
                ? false
                : query.state.data?.some((snapshot) =>
                        ['starting', 'stopping', 'repairing'].includes(snapshot.state),
                    )
                  ? 1000
                  : Date.now() - mountedAt.current < 30_000
                    ? 2500
                    : 15_000,
    });

    useEffect(() => {
        if (firstDataMarked.current) return;
        if (query.data === undefined) return;
        firstDataMarked.current = true;
        perfMark('bots_first_data', { once: true });
    }, [query.data]);

    useDomainEvents((event) => {
        if (event.kind !== 'bot_state_changed') return;
        queryClient.setQueryData<BotActorSnapshot[]>(BOT_SNAPSHOTS_KEY, (old) =>
            updateBotSnapshots(old, event.snapshot),
        );
    });

    return query;
}

export const botSnapshotsKey = BOT_SNAPSHOTS_KEY;
