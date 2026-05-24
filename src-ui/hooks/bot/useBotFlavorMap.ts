// 批量拉所有 Bot 的 backend_type，缓存成 `Record<botId, Flavor>`。
// 调用单个 `list_bot_flavors` Tauri 命令一次性拿全表，避免 N+1。

import { useQuery } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { Flavor } from '../../core/domain/bot/flavor';

const FLAVORS_KEY = ['botFlavors'] as const;

/// 返回 `Record<botId, Flavor>`，未加载完成时返回空对象。
/// 当 botSnapshots 列表的 bot_id 集合（key）发生变化时自动 refetch。
export function useBotFlavorMap(snapshots: { bot_id: string }[]): Record<string, Flavor> {
    // 加入 bot_id 集合到 queryKey，新增 bot 时触发重拉。
    const idsKey = snapshots.map((b) => b.bot_id).sort().join(',');

    const query = useQuery({
        queryKey: [...FLAVORS_KEY, idsKey],
        queryFn: () => botService.listFlavors(),
        // 后端只读 BotConfigRepo.list()，可以稳定缓存到下一次失效。
        staleTime: 60_000,
    });

    return query.data ?? {};
}
