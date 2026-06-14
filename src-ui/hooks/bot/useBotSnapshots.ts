// 实时同步 Bot snapshot 列表的 hook。
// useQuery + bot_state_changed 事件合并 → 单一 source of truth。

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef } from 'react';
import { botService } from '../../core/services/bot.service';
import { useDomainEvents } from '../events/useDomainEvents';
import type { BotActorSnapshot } from '../../core/ipc/types';

const BOT_SNAPSHOTS_KEY = ['botSnapshots'] as const;

export function useBotSnapshots() {
    const queryClient = useQueryClient();
    const mountedAt = useRef(Date.now());

    const query = useQuery<BotActorSnapshot[], Error>({
        queryKey: BOT_SNAPSHOTS_KEY,
        queryFn: botService.listSnapshots,
        // bootstrap 在 setup 里异步 reconcile；首屏 list 常早于 Running，短时轮询对齐卡片
        refetchInterval: () =>
            Date.now() - mountedAt.current < 30_000 ? 2_500 : false,
    });

    useDomainEvents((event) => {
        if (event.kind !== 'bot_state_changed') return;
        queryClient.setQueryData<BotActorSnapshot[]>(BOT_SNAPSHOTS_KEY, (old) => {
            const snap = event.snapshot;
            if (!old?.length) {
                return [snap];
            }
            const idx = old.findIndex((s) => s.bot_id === snap.bot_id);
            if (idx < 0) {
                return [...old, snap];
            }
            return old.map((s) => (s.bot_id === snap.bot_id ? snap : s));
        });
    });

    return query;
}

export const botSnapshotsKey = BOT_SNAPSHOTS_KEY;
