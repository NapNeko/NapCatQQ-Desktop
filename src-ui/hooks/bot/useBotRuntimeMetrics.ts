// Bot 实例运行时指标：订阅共享 catalog（list 批量）+ 单 bot 视图。
// 页面不可见时 catalog 自动降频；对话框可额外 live 刷新单 bot。

import { useCallback, useSyncExternalStore } from 'react';
import { useQuery } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BotRuntimeMetrics } from '../../core/ipc/generated/domain/BotRuntimeMetrics';
import {
    getBotRuntimeMetricsCatalogSnapshot,
    subscribeBotRuntimeMetricsCatalog,
} from './botRuntimeMetricsCatalog';

export interface UseBotRuntimeMetricsOptions {
    /**
     * 为 true 时在订阅 catalog 之外，再按间隔拉单 bot 快照（对话框打开用）。
     * 默认 false：只吃 list 批量结果。
     */
    liveDetail?: boolean;
}

export interface UseBotRuntimeMetricsResult {
    enabled: boolean;
    intervalMs: number;
    retentionDays: number;
    metrics: BotRuntimeMetrics | null;
    /** 尚无任何快照时的首屏加载；后台轮询不会置 true（避免刷新按钮抽搐） */
    loading: boolean;
    refresh: () => Promise<void>;
}

export function useBotRuntimeMetrics(
    botId: string,
    options: UseBotRuntimeMetricsOptions = {},
): UseBotRuntimeMetricsResult {
    const liveDetail = options.liveDetail === true;
    const catalog = useSyncExternalStore(
        subscribeBotRuntimeMetricsCatalog,
        getBotRuntimeMetricsCatalogSnapshot,
        getBotRuntimeMetricsCatalogSnapshot,
    );

    const fromCatalog = botId ? (catalog.byId[botId] ?? null) : null;
    const detail = useQuery({
        queryKey: ['botRuntimeMetrics', botId, catalog.enabled],
        queryFn: async ({ signal }) => {
            const metrics = await botService.getRuntimeMetrics(botId);
            // IPC 不能撤回；切走 Bot 后忽略迟到结果，重新打开时从当前状态采集。
            signal.throwIfAborted();
            return metrics;
        },
        enabled:
            catalog.enabled &&
            !!botId &&
            (liveDetail || (!fromCatalog && !catalog.fetching && catalog.lastFetchAt > 0)),
        refetchInterval: liveDetail ? Math.max(1000, catalog.intervalMs || 3000) : false,
        retry: false,
        staleTime: 0,
        gcTime: 0,
    });
    const fromDetail = detail.data?.bot_id === botId ? detail.data : null;
    // list 与单拉可独立完成，后到的旧采样不能盖住已经显示的新采样。
    const metrics = !catalog.enabled
        ? null
        : fromCatalog && (!fromDetail || fromCatalog.collected_at_ms > fromDetail.collected_at_ms)
          ? fromCatalog
          : (fromDetail ?? fromCatalog);
    const { refetch } = detail;
    const refresh = useCallback(async () => {
        if (!liveDetail || !catalog.enabled || !botId) return;
        // 与轮询共用在途请求；IPC 没有取消能力，不能取消后又发出第二份。
        await refetch({ cancelRefetch: false });
    }, [liveDetail, catalog.enabled, botId, refetch]);

    return {
        enabled: catalog.enabled,
        intervalMs: catalog.intervalMs,
        retentionDays: catalog.retentionDays,
        metrics,
        // 有数据后 catalog.fetching 也不再算 loading，避免 list 轮询抽按钮
        loading: catalog.enabled && metrics == null && (detail.isFetching || catalog.fetching),
        refresh,
    };
}
