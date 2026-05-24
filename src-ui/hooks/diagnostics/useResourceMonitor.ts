// BootstrapPanel 用的 CPU/RAM 抖动指标。
//
// 当前实现：mock，纯前端定时器；返回最新值 + 24 点历史序列（够画 24 段折线）。
// 真接入系统指标后这里会切到 services（diagnosticsService）。
//
// frontend-layering：本 hook 只负责抖动 + 状态管理，不动 IPC。

import { useEffect, useRef, useState } from 'react';

const HISTORY_SIZE = 24;
const TICK_MS = 2000;

export interface ResourcePoint {
    /** 单调递增 tick id，用于折线 dataKey。 */
    t: number;
    /** 0~100 整数百分比 */
    cpu: number;
    /** 0~100 整数百分比 */
    ram: number;
}

export interface ResourceUsage {
    /** 当前最新值。 */
    cpu: number;
    ram: number;
    /** 最近 24 点历史，最旧 → 最新。 */
    history: ResourcePoint[];
}

function clampPercent(v: number): number {
    return Math.max(0, Math.min(100, Math.round(v)));
}

function seedHistory(initialCpu: number, initialRam: number): ResourcePoint[] {
    return Array.from({ length: HISTORY_SIZE }, (_, idx) => ({
        t: idx,
        cpu: initialCpu,
        ram: initialRam,
    }));
}

export function useResourceMonitor(): ResourceUsage {
    const tickRef = useRef(HISTORY_SIZE);
    const [history, setHistory] = useState<ResourcePoint[]>(() => seedHistory(12, 45));

    useEffect(() => {
        const timer = setInterval(() => {
            setHistory((prev) => {
                const last = prev[prev.length - 1];
                const nextCpu = clampPercent(last.cpu + (Math.floor(Math.random() * 7) - 3));
                const nextRam = clampPercent(last.ram + (Math.floor(Math.random() * 3) - 1));
                tickRef.current += 1;
                const next: ResourcePoint = { t: tickRef.current, cpu: nextCpu, ram: nextRam };
                return [...prev.slice(1), next];
            });
        }, TICK_MS);
        return () => clearInterval(timer);
    }, []);

    const latest = history[history.length - 1];
    return {
        cpu: latest.cpu,
        ram: latest.ram,
        history,
    };
}
