// Bot 实例运行时指标：订阅共享 catalog（list 批量）+ 单 bot 视图。
// 页面不可见时 catalog 自动降频；对话框可额外 live 刷新单 bot。

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from 'react';
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
    loading: boolean;
    refresh: () => void;
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

    const fromCatalog = botId ? catalog.byId[botId] ?? null : null;
    const [detailOverride, setDetailOverride] = useState<BotRuntimeMetrics | null>(null);
    const [gapFill, setGapFill] = useState<BotRuntimeMetrics | null>(null);
    const [detailLoading, setDetailLoading] = useState(false);

    // catalog 未含该 bot 时补一次单拉（新启 bot 两轮 list 之间 / 浏览器 mock）
    useEffect(() => {
        if (!catalog.enabled || !botId || fromCatalog || liveDetail) {
            if (fromCatalog) setGapFill(null);
            return;
        }
        if (catalog.fetching || catalog.lastFetchAt === 0) return;
        let cancelled = false;
        void botService
            .getRuntimeMetrics(botId)
            .then((m) => {
                if (!cancelled) setGapFill(m);
            })
            .catch(() => {
                if (!cancelled) setGapFill(null);
            });
        return () => {
            cancelled = true;
        };
    }, [
        catalog.enabled,
        catalog.fetching,
        catalog.lastFetchAt,
        botId,
        fromCatalog,
        liveDetail,
    ]);

    useEffect(() => {
        if (!liveDetail || !catalog.enabled || !botId) {
            setDetailOverride(null);
            return;
        }
        let cancelled = false;
        const tick = async () => {
            try {
                setDetailLoading(true);
                const m = await botService.getRuntimeMetrics(botId);
                if (!cancelled) setDetailOverride(m);
            } catch {
                if (!cancelled) setDetailOverride(null);
            } finally {
                if (!cancelled) setDetailLoading(false);
            }
        };
        void tick();
        const t = window.setInterval(
            () => void tick(),
            Math.max(1000, catalog.intervalMs || 3000),
        );
        return () => {
            cancelled = true;
            window.clearInterval(t);
        };
    }, [liveDetail, catalog.enabled, catalog.intervalMs, botId]);

    const metrics = useMemo(() => {
        if (liveDetail && detailOverride) return detailOverride;
        return fromCatalog ?? gapFill;
    }, [liveDetail, detailOverride, fromCatalog, gapFill]);

    const refresh = useCallback(() => {
        if (liveDetail && botId) {
            void botService.getRuntimeMetrics(botId).then(setDetailOverride).catch(() => {
                setDetailOverride(null);
            });
        }
    }, [liveDetail, botId]);

    return {
        enabled: catalog.enabled,
        intervalMs: catalog.intervalMs,
        retentionDays: catalog.retentionDays,
        metrics,
        loading: catalog.fetching || detailLoading,
        refresh,
    };
}
