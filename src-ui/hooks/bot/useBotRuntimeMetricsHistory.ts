// 趋势图打开时拉历史；切换时间窗口再拉。不按 3s 狂刷。

import { useQuery } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { MetricsHistoryPoint } from '../../core/ipc/generated/domain/MetricsHistoryPoint';
import {
    type MetricsHistoryWindow,
    resolveHistoryWindowBounds,
} from '../../core/domain/bot/runtime-metrics-settings';

export interface UseBotRuntimeMetricsHistoryResult {
    points: MetricsHistoryPoint[];
    /** 尚无 points 时的首拉 / 换窗口；已有数据时静默刷新不置 true */
    loading: boolean;
    error: string | null;
    refresh: () => Promise<void>;
}

export function useBotRuntimeMetricsHistory(
    botId: string,
    window: MetricsHistoryWindow,
    retentionDays: number,
    enabled: boolean,
): UseBotRuntimeMetricsHistoryResult {
    // 拆字段避免对象引用抖动导致无意义重拉
    const range = window.mode === 'preset' ? window.range : '';
    const fromMs = window.mode === 'custom' ? window.fromMs : 0;
    const toMs = window.mode === 'custom' ? window.toMs : 0;
    const followNow = window.mode === 'custom' ? window.followNow : false;
    const query = useQuery({
        queryKey: [
            'botRuntimeMetricsHistory',
            botId,
            window.mode,
            range,
            fromMs,
            toMs,
            followNow,
            retentionDays,
        ],
        queryFn: async () => {
            const bounds = resolveHistoryWindowBounds(window, retentionDays);
            const list = await botService.getRuntimeMetricsHistory(
                botId,
                bounds.fromMs,
                bounds.toMs,
            );
            return Array.isArray(list) ? list : [];
        },
        enabled: enabled && !!botId,
        retry: false,
        staleTime: Infinity,
    });
    return {
        // 旧实现失败时清空 points，不给图喂上一次的点
        points: query.error ? [] : (query.data ?? []),
        loading: query.isPending,
        error: query.error ? query.error.message : null,
        refresh: async () => {
            await query.refetch();
        },
    };
}
