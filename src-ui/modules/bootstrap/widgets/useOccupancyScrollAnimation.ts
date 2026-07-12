import { useEffect, useRef, useState } from 'react';
import type { ResourcePoint } from '../../../hooks/diagnostics/useResourceMonitor';
import { performanceScrollDurationMs } from '../../../core/domain/performance/performanceSettings';
import { valuesFromHistory } from './occupancyChartGeometry';

export interface ScrollAnimState {
    animationSource: number[];
    incoming: number;
    steadySlotCount: number;
    progress: number;
}

export interface OccupancyScrollFrame {
    mode: 'steady' | 'scroll';
    values: number[];
    progress: number;
    scroll: ScrollAnimState | null;
}

/**
 * 对齐 legacy appendValue：每个新 tick 先 slide 稳态再 stop+start 滚入；progress&lt;1 画 N+1，否则画稳态 N。
 */
export function useOccupancyScrollAnimation(
    history: ResourcePoint[],
    dataKey: 'cpu' | 'ram',
    sampleIntervalMs: number,
    motionEnabled: boolean,
): OccupancyScrollFrame {
    const duration = performanceScrollDurationMs(sampleIntervalMs);

    const [steadyValues, setSteadyValues] = useState<number[]>(() =>
        valuesFromHistory(history, dataKey),
    );
    const [scroll, setScroll] = useState<ScrollAnimState | null>(null);

    const lastSeenTRef = useRef<number | null>(null);
    const rafRef = useRef(0);
    const animStartRef = useRef(0);
    const steadyRef = useRef(steadyValues);
    // rAF 期间用 ref 攒 progress，约每 2 帧才 setState，少触发 React 重渲染。
    const scrollPayloadRef = useRef<Omit<ScrollAnimState, 'progress'> | null>(null);
    const lastCommitProgressRef = useRef(-1);
    const frameSkipRef = useRef(0);
    steadyRef.current = steadyValues;

    const latestT = history[history.length - 1]?.t ?? null;

    const stopRaf = () => {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = 0;
        scrollPayloadRef.current = null;
        lastCommitProgressRef.current = -1;
        frameSkipRef.current = 0;
    };

    const startScroll = (
        animationSource: number[],
        incoming: number,
        steadySlotCount: number,
    ) => {
        stopRaf();
        if (!motionEnabled) {
            setScroll(null);
            return;
        }

        animStartRef.current = performance.now();
        scrollPayloadRef.current = { animationSource, incoming, steadySlotCount };
        lastCommitProgressRef.current = 0;
        frameSkipRef.current = 0;
        setScroll({ animationSource, incoming, steadySlotCount, progress: 0 });

        const tick = (now: number) => {
            const payload = scrollPayloadRef.current;
            if (!payload) return;
            if (document.hidden) {
                // 后台不刷 React；回到前台继续从当前时间算 progress。
                rafRef.current = requestAnimationFrame(tick);
                return;
            }
            const p = Math.min(1, (now - animStartRef.current) / duration);
            if (p >= 1) {
                setScroll(null);
                scrollPayloadRef.current = null;
                return;
            }
            // 隔帧提交 + 进度变化阈值，保持滚动观感同时降 React 压力。
            frameSkipRef.current += 1;
            const shouldCommit =
                frameSkipRef.current % 2 === 0 &&
                Math.abs(p - lastCommitProgressRef.current) >= 0.04;
            if (shouldCommit) {
                lastCommitProgressRef.current = p;
                setScroll({ ...payload, progress: p });
            }
            rafRef.current = requestAnimationFrame(tick);
        };
        rafRef.current = requestAnimationFrame(tick);
    };

    useEffect(() => {
        if (latestT === null) {
            lastSeenTRef.current = null;
            setSteadyValues([]);
            steadyRef.current = [];
            setScroll(null);
            stopRaf();
            return;
        }

        if (lastSeenTRef.current === null) {
            lastSeenTRef.current = latestT;
            const initial = valuesFromHistory(history, dataKey);
            steadyRef.current = initial;
            setSteadyValues(initial);
            return;
        }

        if (latestT === lastSeenTRef.current) {
            return;
        }

        lastSeenTRef.current = latestT;
        const incoming = history[history.length - 1][dataKey];
        const prev = steadyRef.current;
        if (prev.length < 1) {
            const initial = valuesFromHistory(history, dataKey);
            steadyRef.current = initial;
            setSteadyValues(initial);
            return;
        }

        const animationSource = [...prev];
        const nextSteady =
            prev.length > 1 ? [...prev.slice(1), incoming] : [incoming];
        steadyRef.current = nextSteady;
        setSteadyValues(nextSteady);

        startScroll(animationSource, incoming, nextSteady.length);
    }, [latestT, dataKey, history, duration, motionEnabled]);

    useEffect(() => () => stopRaf(), []);

    if (scroll !== null && motionEnabled && scroll.progress < 1) {
        return {
            mode: 'scroll',
            values: [...scroll.animationSource, scroll.incoming],
            progress: scroll.progress,
            scroll,
        };
    }

    return {
        mode: 'steady',
        values: steadyValues,
        progress: 1,
        scroll: null,
    };
}