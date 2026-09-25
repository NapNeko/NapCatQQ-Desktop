import { useEffect, useRef, useState } from 'react';
import type { ResourcePoint } from '../../../hooks/diagnostics/useResourceMonitor';
import { valuesFromHistory } from './occupancyChartGeometry';

// 对齐 legacy appendValue：每来一个新采样，稳态窗口左移一格，新点从右边滚进来。
// 这里只记这一段从哪滚到哪、什么时候开始；滚动本身由 OccupancyChart 平移整条曲线来做，
// 采样之间不再触发重渲。
export interface OccupancySegment {
    /** 稳态 N 个值，也就是这一段滚完之后的样子 */
    steady: number[];
    /** 滚入前的稳态；null 表示这一段不滚（首个采样、关了动效） */
    scrollFrom: number[] | null;
    /** 滚入开始时的 performance.now()；尺寸变了要重建动画、悬停要取进度，都从这里算 */
    startedAt: number;
}

const EMPTY: OccupancySegment = { steady: [], scrollFrom: null, startedAt: 0 };

export function useOccupancySegment(
    history: ResourcePoint[],
    dataKey: 'cpu' | 'ram',
    motionEnabled: boolean,
): OccupancySegment {
    const [segment, setSegment] = useState<OccupancySegment>(() => ({
        ...EMPTY,
        steady: valuesFromHistory(history, dataKey),
    }));
    const lastSeenTRef = useRef<number | null>(null);
    const steadyRef = useRef(segment.steady);
    const latestT = history[history.length - 1]?.t ?? null;

    useEffect(() => {
        if (latestT === null) {
            lastSeenTRef.current = null;
            steadyRef.current = [];
            setSegment(EMPTY);
            return;
        }
        if (latestT === lastSeenTRef.current) return;
        const first = lastSeenTRef.current === null;
        lastSeenTRef.current = latestT;

        const prev = steadyRef.current;
        if (first || prev.length < 1) {
            const initial = valuesFromHistory(history, dataKey);
            steadyRef.current = initial;
            setSegment({ ...EMPTY, steady: initial });
            return;
        }

        const incoming = history[history.length - 1][dataKey];
        const next = prev.length > 1 ? [...prev.slice(1), incoming] : [incoming];
        steadyRef.current = next;
        setSegment({
            steady: next,
            scrollFrom: motionEnabled ? prev : null,
            startedAt: performance.now(),
        });
    }, [latestT, dataKey, history, motionEnabled]);

    return segment;
}
