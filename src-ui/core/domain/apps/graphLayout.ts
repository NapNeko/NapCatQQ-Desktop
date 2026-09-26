// 记忆图谱的布局：力导向（点之间互相推开、边像弹簧拉住、整体往中心收一点），一次算完再画，
// 不边算边动：上百个点边动边重绘在低配机上会卡，而且每次打开形状都不一样。
// 起始位置按点的先后排在螺旋上，同一份数据每次算出来都一样。

export type LayoutNode = { id: string };
export type LayoutEdge = { source: string; target: string };
export type Point = { x: number; y: number };

const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

/**
 * Fruchterman–Reingold。k 是理想边长；gravity 把不连通的小团往中间拢，免得飘出画面。
 * 点数在几百以内，两两排斥直接算，不上四叉树
 */
export function forceLayout(
    nodes: readonly LayoutNode[],
    edges: readonly LayoutEdge[],
    opts: { k?: number; iterations?: number; gravity?: number } = {},
): Map<string, Point> {
    const n = nodes.length;
    const out = new Map<string, Point>();
    if (n === 0) return out;
    const k = opts.k ?? 60;
    const iterations = opts.iterations ?? (n > 200 ? 220 : 320);
    const gravity = opts.gravity ?? 0.06;

    const index = new Map(nodes.map((node, i) => [node.id, i]));
    const xs = new Float64Array(n);
    const ys = new Float64Array(n);
    for (let i = 0; i < n; i++) {
        const r = k * 0.9 * Math.sqrt(i + 0.5);
        xs[i] = r * Math.cos(i * GOLDEN_ANGLE);
        ys[i] = r * Math.sin(i * GOLDEN_ANGLE);
    }
    const links: [number, number][] = [];
    for (const e of edges) {
        const a = index.get(e.source);
        const b = index.get(e.target);
        if (a !== undefined && b !== undefined && a !== b) links.push([a, b]);
    }

    const dx = new Float64Array(n);
    const dy = new Float64Array(n);
    let temperature = k * Math.sqrt(n) * 0.5;
    const cooling = temperature / iterations;
    for (let step = 0; step < iterations; step++) {
        dx.fill(0);
        dy.fill(0);
        for (let i = 0; i < n; i++) {
            for (let j = i + 1; j < n; j++) {
                let vx = xs[i] - xs[j];
                let vy = ys[i] - ys[j];
                let d2 = vx * vx + vy * vy;
                if (d2 < 0.01) {
                    // 叠在一起时按下标错开一点，别除以零
                    vx = (i - j) * 0.1;
                    vy = 0.1;
                    d2 = vx * vx + vy * vy;
                }
                const f = (k * k) / d2;
                dx[i] += vx * f;
                dy[i] += vy * f;
                dx[j] -= vx * f;
                dy[j] -= vy * f;
            }
        }
        for (const [a, b] of links) {
            const vx = xs[a] - xs[b];
            const vy = ys[a] - ys[b];
            const d = Math.sqrt(vx * vx + vy * vy) || 0.01;
            const f = d / k;
            dx[a] -= vx * f;
            dy[a] -= vy * f;
            dx[b] += vx * f;
            dy[b] += vy * f;
        }
        for (let i = 0; i < n; i++) {
            dx[i] -= xs[i] * gravity;
            dy[i] -= ys[i] * gravity;
            const len = Math.sqrt(dx[i] * dx[i] + dy[i] * dy[i]);
            if (len > 0) {
                const move = Math.min(len, temperature);
                xs[i] += (dx[i] / len) * move;
                ys[i] += (dy[i] / len) * move;
            }
        }
        temperature = Math.max(k * 0.02, temperature - cooling);
    }

    let cx = 0;
    let cy = 0;
    for (let i = 0; i < n; i++) {
        cx += xs[i];
        cy += ys[i];
    }
    cx /= n;
    cy /= n;
    nodes.forEach((node, i) => out.set(node.id, { x: xs[i] - cx, y: ys[i] - cy }));
    return out;
}

export type Bounds = { minX: number; minY: number; maxX: number; maxY: number };

export function boundsOf(points: Iterable<Point>): Bounds {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const p of points) {
        minX = Math.min(minX, p.x);
        minY = Math.min(minY, p.y);
        maxX = Math.max(maxX, p.x);
        maxY = Math.max(maxY, p.y);
    }
    if (!Number.isFinite(minX)) return { minX: -1, minY: -1, maxX: 1, maxY: 1 };
    return { minX, minY, maxX, maxY };
}

/** 把整张图放进 width × height（留 padding），给平移和缩放的起点 */
export function fitView(b: Bounds, width: number, height: number, padding = 48): { x: number; y: number; k: number } {
    const w = Math.max(1, b.maxX - b.minX);
    const h = Math.max(1, b.maxY - b.minY);
    const k = Math.min(2, Math.max(0.1, Math.min((width - padding * 2) / w, (height - padding * 2) / h)));
    return { k, x: width / 2 - ((b.minX + b.maxX) / 2) * k, y: height / 2 - ((b.minY + b.maxY) / 2) * k };
}
