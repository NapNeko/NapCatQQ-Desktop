export interface ChartPoint {
    x: number;
    y: number;
}

export function valuesFromHistory<T extends { cpu: number; ram: number }>(
    history: T[],
    dataKey: 'cpu' | 'ram',
): number[] {
    return history.map((p) => p[dataKey]);
}

export function interpolateAtX(a: ChartPoint, b: ChartPoint, clipX: number): ChartPoint {
    if (a.x === b.x) return { x: clipX, y: b.y };
    const r = (clipX - a.x) / (b.x - a.x);
    return { x: clipX, y: a.y + (b.y - a.y) * r };
}

export function clipDisplayPoints(points: ChartPoint[], left: number, right: number): ChartPoint[] {
    if (points.length < 2) return points;
    const out: ChartPoint[] = [];
    if (points[0].x < left) {
        out.push(interpolateAtX(points[0], points[1], left));
    }
    for (const p of points) {
        if (p.x >= left && p.x <= right) out.push(p);
    }
    const last = points[points.length - 1];
    if (last.x > right) {
        out.push(interpolateAtX(points[points.length - 2], last, right));
    }
    return out.length >= 2 ? out : points;
}

export function buildSmoothPath(points: ChartPoint[]): string {
    if (points.length === 0) return '';
    if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;
    let d = `M ${points[0].x} ${points[0].y}`;
    for (let i = 1; i < points.length; i++) {
        const prev = points[i - 1];
        const cur = points[i];
        const midX = (prev.x + cur.x) / 2;
        d += ` C ${midX} ${prev.y}, ${midX} ${cur.y}, ${cur.x} ${cur.y}`;
    }
    return d;
}

export function buildAreaPath(linePath: string, points: ChartPoint[], bottom: number, left: number): string {
    if (points.length === 0) return '';
    const last = points[points.length - 1];
    return `${linePath} L ${last.x} ${bottom} L ${left} ${bottom} Z`;
}

export function steadyPoints(values: number[], innerW: number, innerH: number): ChartPoint[] {
    const n = values.length;
    if (n === 0) return [];
    const stepX = n > 1 ? innerW / (n - 1) : innerW;
    return values.map((v, i) => ({
        x: n > 1 ? i * stepX : innerW,
        y: innerH * (1 - v / 100),
    }));
}

/**
 * legacy _OccupancyCanvas._build_points：稳态 N 点用 width/(N-1)；滚入时绘 N+1 点，步长按稳态槽位 N 计算，左移 stepX*progress。
 */
export function scrollPoints(
    animationSource: number[],
    incoming: number,
    progress: number,
    steadySlotCount: number,
    innerW: number,
    innerH: number,
): { values: number[]; points: ChartPoint[] } {
    const values = [...animationSource, incoming];
    const nSlots = Math.max(1, steadySlotCount);
    const stepX = nSlots > 1 ? innerW / (nSlots - 1) : innerW;
    const shift = stepX * Math.max(0, Math.min(1, progress));
    const points = values.map((v, index) => ({
        x: stepX * index - shift,
        y: innerH * (1 - v / 100),
    }));
    return { values, points };
}

export function pickHover(
    clipped: ChartPoint[],
    values: number[],
    hoverX: number,
): { p: ChartPoint; value: number } | null {
    if (clipped.length < 2) return null;
    if (hoverX <= clipped[0].x) return { p: clipped[0], value: values[0] };
    if (hoverX >= clipped[clipped.length - 1].x) {
        return { p: clipped[clipped.length - 1], value: values[values.length - 1] };
    }
    for (let i = 1; i < clipped.length; i++) {
        const a = clipped[i - 1];
        const b = clipped[i];
        if (hoverX > b.x) continue;
        const r = (hoverX - a.x) / (b.x - a.x);
        const y = a.y + (b.y - a.y) * r;
        const va = values[i - 1];
        const vb = values[i];
        return { p: { x: hoverX, y }, value: va + (vb - va) * r };
    }
    return null;
}