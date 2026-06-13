// 批量拉所有 Bot 的完整 BotConfig，缓存成 `Record<botId, BotConfig>`。
//
// 没有专门的 list_bot_configs 后端命令，前端走 useQueries 并行 N 次
// `get_bot_config`。BotCard 数量上限就 4 个，N+1 完全可接受。
//
// staleTime 60s：BotConfig 只会在用户主动改配置 / 删 bot 时变化，列表页内
// 看到的元数据不必紧跟。upsertBotConfig / deleteBotConfig mutation 已经
// 通过 botSnapshotsKey 同步刷新列表，需要时手动 invalidate 这里即可。

import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';

export const botConfigsKeyPrefix = ['botConfig'] as const;

export function botConfigKey(botId: string): readonly unknown[] {
    return [...botConfigsKeyPrefix, botId];
}

/// 返回 `Record<botId, BotConfig | null>`：null 表示后端读不到该配置（已删）。
/// 未加载完成的 bot 会缺 key。
export function useBotConfigsMap(
    snapshots: { bot_id: string }[],
): Record<string, BotConfig | null> {
    const queries = useQueries({
        queries: snapshots.map((b) => ({
            queryKey: botConfigKey(b.bot_id),
            queryFn: () => botService.getConfig(b.bot_id),
            staleTime: 60_000,
        })),
    });

    const dataSignature = useMemo(
        () =>
            queries
                .map((q, i) => {
                    const id = snapshots[i]?.bot_id ?? '';
                    return `${id}:${q.status}:${q.dataUpdatedAt ?? 0}`;
                })
                .join('|'),
        [queries, snapshots],
    );

    return useMemo(() => {
        const out: Record<string, BotConfig | null> = {};
        queries.forEach((q, i) => {
            const id = snapshots[i]?.bot_id;
            if (!id) return;
            if (q.data !== undefined) out[id] = q.data;
        });
        return out;
    }, [dataSignature, queries, snapshots]);
}
