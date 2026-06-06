// 概览 CPU/RAM 采样。enabled=false 时不启定时器、不 invoke。

import { useCallback, useEffect, useRef, useState } from 'react';
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

function clampPercent(v: number): number {
    return Math.max(0, Math.min(100, Math.round(v)));
}

function emptyUsage(): ResourceUsage {
    return {
        cpu: 0,
        ram: 0,
        history: [],
        status: 'idle',
        errorMessage: null,
    };
}

export function useResourceMonitor(options: UseResourceMonitorOptions): ResourceUsage {
    const { enabled, intervalMs } = options;
    const tickRef = useRef(0);
    const [state, setState] = useState<ResourceUsage>(() =>
        enabled ? { ...emptyUsage(), status: 'warming' } : emptyUsage(),
    );

    const applySnapshot = useCallback((cpu: number, ram: number) => {
        setState((prev) => {
            if (prev.history.length === 0) {
                const nextHistory = Array.from(
                    { length: PERFORMANCE_MONITOR_HISTORY_SIZE },
                    (_, i) => ({
                        t: i + 1,
                        cpu,
                        ram,
                    }),
                );
                tickRef.current = PERFORMANCE_MONITOR_HISTORY_SIZE;
                return {
                    cpu,
                    ram,
                    history: nextHistory,
                    status: 'ready',
                    errorMessage: null,
                };
            }
            tickRef.current += 1;
            const point: ResourcePoint = {
                t: tickRef.current,
                cpu,
                ram,
            };
            const nextHistory =
                prev.history.length >= PERFORMANCE_MONITOR_HISTORY_SIZE
                    ? [...prev.history.slice(1), point]
                    : [...prev.history, point];
            return {
                cpu,
                ram,
                history: nextHistory,
                status: 'ready',
                errorMessage: null,
            };
        });
    }, []);

    const sample = useCallback(
        async (bootstrap: boolean) => {
            try {
                const snap = await systemMetricsService.snapshot({ bootstrap });
                const cpu = clampPercent(snap.cpuPercent);
                const ram = clampPercent(snap.ramPercent);
                applySnapshot(cpu, ram);
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                setState((prev) => ({
                    ...prev,
                    status: 'error',
                    errorMessage: msg,
                }));
            }
        },
        [applySnapshot],
    );

    useEffect(() => {
        if (!enabled) {
            setState(emptyUsage());
            tickRef.current = 0;
            return;
        }
        if (!isTauri) {
            setState({
                cpu: 0,
                ram: 0,
                history: [],
                status: 'error',
                errorMessage: '浏览器预览无法读取系统指标',
            });
            return;
        }

        setState((prev) => ({ ...prev, status: 'warming', errorMessage: null }));
        let cancelled = false;
        const tick = (bootstrap: boolean) => {
            if (!cancelled) void sample(bootstrap);
        };
        tick(true);
        const ms = clampPerformanceMonitorIntervalMs(intervalMs);
        const timer = setInterval(() => tick(false), ms);
        return () => {
            cancelled = true;
            clearInterval(timer);
        };
    }, [enabled, intervalMs, sample]);

    return state;
}