// 趋势图打开时拉历史；切换时间窗口再拉。不按 3s 狂刷。

import { useCallback, useEffect, useState } from 'react';
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
    const [points, setPoints] = useState<MetricsHistoryPoint[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async (opts?: { silent?: boolean }) => {
        if (!botId || !enabled) {
            setPoints([]);
            return;
        }
        const silent = opts?.silent === true;
        if (!silent) setLoading(true);
        setError(null);
        try {
            const { fromMs, toMs } = resolveHistoryWindowBounds(
                window,
                retentionDays,
            );
            const list = await botService.getRuntimeMetricsHistory(
                botId,
                fromMs,
                toMs,
            );
            setPoints(Array.isArray(list) ? list : []);
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
            setPoints([]);
        } finally {
            if (!silent) setLoading(false);
        }
    }, [
        botId,
        enabled,
        retentionDays,
        // 拆字段避免对象引用抖动导致无意义重拉
        window.mode,
        window.mode === 'preset' ? window.range : '',
        window.mode === 'custom' ? window.fromMs : 0,
        window.mode === 'custom' ? window.toMs : 0,
        window.mode === 'custom' ? window.followNow : false,
    ]);

    useEffect(() => {
        void load();
    }, [load]);

    return {
        points,
        loading,
        error,
        refresh: () => load({ silent: true }),
    };
}
