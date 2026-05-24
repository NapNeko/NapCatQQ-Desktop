// 实时同步 Bot snapshot 列表的 hook。
// useQuery + bot_state_changed 事件合并 → 单一 source of truth。

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import { useDomainEvents } from '../events/useDomainEvents';
import type { BotActorSnapshot } from '../../core/ipc/types';

const BOT_SNAPSHOTS_KEY = ['botSnapshots'] as const;

export function useBotSnapshots() {
    const queryClient = useQueryClient();

    const query = useQuery<BotActorSnapshot[], Error>({
        queryKey: BOT_SNAPSHOTS_KEY,
        queryFn: botService.listSnapshots,
    });

    useDomainEvents((event) => {
        if (event.kind !== 'bot_state_changed') return;
        queryClient.setQueryData<BotActorSnapshot[]>(BOT_SNAPSHOTS_KEY, (old) => {
            if (!old) return old;
            return old.map((snap) =>
                snap.bot_id === event.snapshot.bot_id ? event.snapshot : snap,
            );
        });
    });

    return query;
}

export const botSnapshotsKey = BOT_SNAPSHOTS_KEY;
