// 概览 CPU/RAM 采样。enabled=false 时不 invoke、不留历史。
// 快照拉取走 useQuery(['systemMetrics']) + refetchInterval；曲线窗口留在本地累积：
// react-query 对内容相同的快照做结构共享（引用不变），拿 data 变化当「来了新采样」
// 会让平稳读数时图表停走，所以每个真实请求在 queryFn 里往历史推一个点。

import { useCallback, useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_HISTORY_SIZE,
} from '../../core/domain/performance/performanceSettings';
import { systemMetricsService } from '../../core/services/system-metrics.service';
import { isTauri } from '../../core/ipc/transport';

export interface ResourcePoint {
    t: number;
    cpu: number;
    ram: number;
}

export type ResourceMonitorStatus = 'idle' | 'warming' | 'ready' | 'error';

export interface ResourceUsage {
    cpu: number;
    ram: number;
    history: ResourcePoint[];
    status: ResourceMonitorStatus;
    errorMessage: string | null;
}

export interface UseResourceMonitorOptions {
    enabled: boolean;
    intervalMs: number;
}

const SYSTEM_METRICS_KEY = ['systemMetrics'] as const;

interface HistoryWindow {
    cpu: number;
    ram: number;
    history: ResourcePoint[];
}

function emptyHistory(): HistoryWindow {
    return { cpu: 0, ram: 0, history: [] };
}

function emptyUsage(): ResourceUsage {
    return { cpu: 0, ram: 0, history: [], status: 'idle', errorMessage: null };
}

function clampPercent(v: number): number {
    return Math.max(0, Math.min(100, Math.round(v)));
}

export function useResourceMonitor(options: UseResourceMonitorOptions): ResourceUsage {
    const { enabled, intervalMs } = options;
    const client = useQueryClient();
    const tickRef = useRef(0);
    // 首次 / 重新启用 / 换间隔后的第一枪带 bootstrap，让后端跳过 CPU 最小间隔等待
    const firstSampleRef = useRef(true);
    // 旧实现的 cancelled 语义：停采样后在途回包不许再写历史
    const samplingRef = useRef(enabled && isTauri);
    const [sample, setSample] = useState<HistoryWindow>(emptyHistory);

    const applySnapshot = useCallback((cpu: number, ram: number) => {
        if (!samplingRef.current) return;
        setSample((prev) => {
            if (prev.history.length === 0) {
                // 首帧把整条曲线填满同一个读数，避免图表左半边空着（legacy 同款）
                const nextHistory = Array.from(
                    { length: PERFORMANCE_MONITOR_HISTORY_SIZE },
                    (_, i) => ({ t: i + 1, cpu, ram }),
                );
                tickRef.current = PERFORMANCE_MONITOR_HISTORY_SIZE;
                return { cpu, ram, history: nextHistory };
            }
            tickRef.current += 1;
            const point: ResourcePoint = { t: tickRef.current, cpu, ram };
            const nextHistory =
                prev.history.length >= PERFORMANCE_MONITOR_HISTORY_SIZE
                    ? [...prev.history.slice(1), point]
                    : [...prev.history, point];
            return { cpu, ram, history: nextHistory };
        });
    }, []);

    useEffect(() => {
        samplingRef.current = enabled && isTauri;
        if (enabled) return;
        // 关掉监控：清历史 + tick 归零，并作废缓存里挂着的旧错误态，
        // 重新启用必须重新拉并回到 warming（对齐旧实现）
        setSample(emptyHistory());
        tickRef.current = 0;
        firstSampleRef.current = true;
        void client.removeQueries({ queryKey: SYSTEM_METRICS_KEY });
    }, [enabled, client]);

    const query = useQuery({
        queryKey: SYSTEM_METRICS_KEY,
        queryFn: async () => {
            const bootstrap = firstSampleRef.current;
            firstSampleRef.current = false;
            const snap = await systemMetricsService.snapshot({ bootstrap });
            const cpu = clampPercent(snap.cpuPercent);
            const ram = clampPercent(snap.ramPercent);
            applySnapshot(cpu, ram);
            return { cpu, ram };
        },
        // 浏览器预览不发请求：旧实现直接落 error 态，service 的 mock 读数不进曲线
        enabled: enabled && isTauri,
        refetchInterval: clampPerformanceMonitorIntervalMs(intervalMs),
        // 旧实现是裸 setInterval：窗口失焦照常采样，不能让 react-query 省掉后台轮次
        refetchIntervalInBackground: true,
        staleTime: 0,
        // 采样点时效即弃：卸载（路由切走）后不留旧快照，下次挂载从头开始
        gcTime: 0,
    });

    const lastIntervalRef = useRef<number | null>(null);
    useEffect(() => {
        if (!enabled || !isTauri) return;
        // 换间隔立即补一枪 bootstrap 采样，对齐旧实现「effect 重跑就 tick(true)」；
        // 挂载首轮不算换间隔，交给 useQuery 的首发，免得白多一次 invoke
        if (lastIntervalRef.current !== null && lastIntervalRef.current !== intervalMs) {
            firstSampleRef.current = true;
            void client.invalidateQueries({ queryKey: SYSTEM_METRICS_KEY });
        }
        lastIntervalRef.current = intervalMs;
    }, [enabled, intervalMs, client]);

    if (!enabled) return emptyUsage();
    if (!isTauri) {
        return {
            cpu: 0,
            ram: 0,
            history: [],
            status: 'error',
            errorMessage: '浏览器预览无法读取系统指标',
        };
    }

    // 出错时保留旧读数（旧实现 catch 只改 status / errorMessage），下一枪成功自动回 ready
    if (query.isError) {
        return {
            ...sample,
            status: 'error',
            errorMessage: query.error instanceof Error ? query.error.message : String(query.error),
        };
    }
    return {
        ...sample,
        status: sample.history.length > 0 ? 'ready' : 'warming',
        errorMessage: null,
    };
}
