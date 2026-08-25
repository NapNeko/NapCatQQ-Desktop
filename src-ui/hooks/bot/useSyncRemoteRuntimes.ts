// 列表打开时把远端仍在跑的 QQ/容器接到 Actor，避免只在导入/冷启动 reconcile 一次。

import { useEffect, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import { isRuntimeTargetLocal } from '../../core/domain/bot/runtime-target';
import { botSnapshotsKey } from './useBotSnapshots';
import type { BotActorSnapshot } from '../../core/ipc/types';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';

export function useSyncRemoteRuntimes(
    snapshots: BotActorSnapshot[],
    configByBot: Record<string, BotConfig | null>,
) {
    const queryClient = useQueryClient();
    const started = useRef(false);

    useEffect(() => {
        if (started.current) return;
        if (snapshots.length === 0) return;
        const waiting = snapshots.some(
            (s) =>
                (s.state === 'stopped' || s.state === 'crashed') &&
                configByBot[s.bot_id] === undefined,
        );
        if (waiting) return;

        const ids = snapshots
            .filter((s) => s.state === 'stopped' || s.state === 'crashed')
            .filter((s) => {
                const cfg = configByBot[s.bot_id];
                return cfg != null && !isRuntimeTargetLocal(cfg.bot.runtime_target);
            })
            .map((s) => s.bot_id);
        started.current = true;
        if (ids.length === 0) return;

        void botService
            .reconcileRuntimes(ids)
            .then((attached) => {
                if (attached.length > 0) {
                    void queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
                }
            })
            .catch(() => {
                started.current = false;
            });
    }, [snapshots, configByBot, queryClient]);
}
