// 吉祥物 SVG（cat_girl.svg）的骨架表：脸部部件按 node-id 点名，身体部件按区域切层。
//
// 素材是描摹导出的：一张全身黑色剪影垫底，彩色块盖在上面露出黑边；头发、外套、
// 阴影都是跨部位的大 path，没法按 path 归到某个身体部位。所以身体分层走「裁切窗口」：
// 每层复制一份画（只保留和窗口相交的 path），用 clipPath 切出自己的区域，
// 接缝选在颈部 / 猫和袖子之间 / 鞋底这些静止时完全重合、动起来只差一两像素的地方。
// 坐标全是 viewBox 单位（1024 x 1536），是拿 1:1 渲染逐行扫像素定下来的；换素材要重标。

export const MASCOT_PARTS = {
    /** 女孩两只眼睛（左眼两层 + 右眼）。 */
    eyes: ['504', '508', '631', '639', '506', '635'],
    /** 两边腮红。 */
    cheeks: ['516', '657', '510', '651'],
    /** 抱在怀里那只猫的眼睛。 */
    heldCatEyes: ['556', '689', '558', '694'],
    /** 地上那只猫的眼睛。 */
    floorCatEyes: ['572', '831', '574', '836'],
    /** 领口的红领结。 */
    bow: ['352', '671'],
} as const;

export type MascotPart = keyof typeof MASCOT_PARTS;

export type MascotRig = Record<MascotPart, SVGGraphicsElement[]>;

export function rigMascot(root: ParentNode): MascotRig {
    const out = {} as MascotRig;
    for (const part of Object.keys(MASCOT_PARTS) as MascotPart[]) {
        const selector = MASCOT_PARTS[part].map((id) => `[node-id="${id}"]`).join(',');
        out[part] = Array.from(root.querySelectorAll<SVGGraphicsElement>(selector));
    }
    return out;
}

/** [x0, y0, x1, y1] */
export type Rect = readonly [number, number, number, number];
/** 扁平的 x y x y ...，至少三个点。 */
export type Poly = readonly number[];
export type Shape = Rect | Poly;

export type MascotLayer = 'ground' | 'body' | 'heldCat' | 'head' | 'floorCat';

interface LayerSpec {
    name: MascotLayer;
    shapes: readonly Shape[];
    /** 在 figure 组里的层会跟着整个人一起跳；地面和地上的猫不跟。 */
    inFigure: boolean;
}

// 关键分界（viewBox 单位）：
//   403  左侧头发发尾（y≈402）和怀里猫的头顶（y≈404）之间
//   428  脖子：领结（≤418）以上归头，衣领（≥430）以下归身
//   462  怀里猫的右边缘（≈456）和衣领左缘之间
//   622  猫身底部（613）以下是袖子
//   372 / 1090  地上那只猫的右缘（≈365，再往右是鞋尖 376）和头顶；身体层在这块让开
//   1398 鞋底 / 猫脚 与地面阴影的交界；1414 鞋底白边结束
//
// 头和怀里猫之间不是直线：头发左下缘是条斜线（(322,370) 到发尾 (437,402)），
// 窗口沿着它往下让 4 单位切，猫举起来时耳朵是钻进头发底下，不是被切平。
const HAIR_EDGE = [296, 374, 322, 374, 329, 379, 340, 384, 360, 389, 391, 394, 411, 399, 429, 404, 440, 406, 462, 406];
const HAIR_EDGE_REVERSED = (() => {
    const out: number[] = [];
    for (let i = HAIR_EDGE.length - 2; i >= 0; i -= 2) out.push(HAIR_EDGE[i], HAIR_EDGE[i + 1]);
    return out;
})();

// 脚下：阴影上缘不是平的，两鞋之间高到 1394、右鞋右边 1396、两端 1398。身体层在鞋以外的地方
// 只到 1392，免得抬脚时把阴影上缘那两三单位一起带走；两只鞋各用一块贴着鞋型的多边形往下包到鞋底
// （左脚在后、离地高，白边到 1403；右脚在前，到 1413），两头削角，别把鞋边外的阴影带走。
// 底边只包到白边下沿：再往下两三单位的黑描边留在地上（黑对黑看不见），比带走一条平底黑块好看。
const SOLE_FOOTPRINTS: readonly Poly[] = [
    [376, 1392, 508, 1392, 510, 1398, 506, 1404, 380, 1404, 375, 1400],
    [530, 1392, 688, 1392, 692, 1400, 682, 1414, 540, 1414, 530, 1406],
];

const LAYERS: readonly LayerSpec[] = [
    // 地上那只猫的窗口到 1406，猫那一列的地面从 1398 起，别和它重叠
    { name: 'ground', shapes: [[0, 1398, 372, 1536], [372, 1392, 1024, 1536]], inFigure: false },
    {
        name: 'body',
        shapes: [
            [0, 403, 296, 1090],
            [296, 622, 462, 1090],
            [372, 1090, 462, 1392],
            [462, 428, 1024, 1392],
            ...SOLE_FOOTPRINTS,
        ],
        inFigure: true,
    },
    { name: 'heldCat', shapes: [[...HAIR_EDGE, 462, 622, 296, 622]], inFigure: true },
    {
        name: 'head',
        shapes: [[0, 0, 1024, 0, 1024, 428, 462, 428, ...HAIR_EDGE_REVERSED, 296, 403, 0, 403]],
        inFigure: true,
    },
    { name: 'floorCat', shapes: [[100, 1090, 372, 1406]], inFigure: false },
];

// 鞋底、猫脚抬起来后，地面层里留下的是它们的静止副本（白边）。从 1398 往下这几块一定在阴影里，
// 直接涂剪影色就等于把影子补齐；静止时身体层盖在上面，看不出来。1392–1398 那一小截白边不敢盖
// （盖到阴影外面就是永久的黑边），抬脚时会留一线白，比常驻的黑边划算。
const SILHOUETTE = '#0f121d';
const GROUND_COVERS: readonly Shape[] = [
    [375, 1401, 377, 1398, 506, 1398, 508, 1401, 506, 1406, 377, 1406],
    [530, 1405, 536, 1398, 679, 1398, 685, 1405, 679, 1416, 536, 1416],
    [225, 1398, 372, 1407],
];

/** 各关节的转轴（viewBox 单位），配 gsap 的 svgOrigin 用。 */
export const MASCOT_PIVOTS = {
    neck: '545 428',
    feet: '545 1412',
    heldCat: '380 620',
    floorCat: '300 1406',
    shadow: '458 1416',
} as const;

const PATH_RE = /<path\b[^>]*>/g;
const NUM_RE = /-?\d*\.?\d+(?:e[-+]?\d+)?/gi;

/// 把 d 里所有数字按 (x, y) 交替取极值。控制点也算进去，所以是偏大的包围盒，
/// 用来判断「和窗口相交」正好够（宁可多留 path 交给 clip，不能漏）。
export function approxBBox(d: string): Rect | null {
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    let i = 0;
    for (const match of d.matchAll(NUM_RE)) {
        const v = Number(match[0]);
        if (i % 2 === 0) {
            if (v < x0) x0 = v;
            if (v > x1) x1 = v;
        } else {
            if (v < y0) y0 = v;
            if (v > y1) y1 = v;
        }
        i += 1;
    }
    return i >= 2 ? [x0, y0, x1, y1] : null;
}

function intersects(a: Rect, b: Rect, margin = 1): boolean {
    return a[0] <= b[2] + margin && a[2] >= b[0] - margin && a[1] <= b[3] + margin && a[3] >= b[1] - margin;
}

function isRect(shape: Shape): shape is Rect {
    return shape.length === 4;
}

function shapeBBox(shape: Shape): Rect {
    if (isRect(shape)) return shape;
    const xs = shape.filter((_, i) => i % 2 === 0);
    const ys = shape.filter((_, i) => i % 2 === 1);
    return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

function rectMarkup([x0, y0, x1, y1]: Rect, extra = ''): string {
    return `<rect x="${x0}" y="${y0}" width="${x1 - x0}" height="${y1 - y0}"${extra}/>`;
}

function shapeMarkup(shape: Shape, extra = ''): string {
    return isRect(shape) ? rectMarkup(shape, extra) : `<polygon points="${shape.join(' ')}"${extra}/>`;
}

/**
 * 把已换色的 cat_girl.svg 文本重组成分层可动的 SVG。
 * idPrefix 保证同页多个实例的 clipPath id 不撞。
 */
export function buildRiggedMascotMarkup(svgText: string, idPrefix: string): string {
    const paths: { markup: string; bbox: Rect | null }[] = [];
    for (const match of svgText.matchAll(PATH_RE)) {
        // 素材是 <path ...></path> 写法，只抓开标签就得自己闭合，不然后面的 path 全嵌进第一条里不画。
        const open = match[0];
        const markup = open.endsWith('/>') ? open : `${open.slice(0, -1)}/>`;
        const d = / d="([^"]*)"/.exec(open)?.[1] ?? '';
        paths.push({ markup, bbox: approxBBox(d) });
    }

    const clipId = (name: string) => `${idPrefix}-${name}`;
    const defs = LAYERS.map(
        (layer) => `<clipPath id="${clipId(layer.name)}">${layer.shapes.map((s) => shapeMarkup(s)).join('')}</clipPath>`,
    ).join('');

    // 素材里有一条整个落在画布左侧外面的废 path（x 在 -1024..0），先扔掉。
    const onCanvas = paths.filter((p) => p.bbox !== null && p.bbox[2] > 0 && p.bbox[0] < 1024 && p.bbox[3] > 0 && p.bbox[1] < 1536);

    const groundPatches = GROUND_COVERS.map((s) => shapeMarkup(s, ` fill="${SILHOUETTE}"`)).join('');

    const layerMarkup = (layer: LayerSpec): string => {
        const bounds = layer.shapes.map(shapeBBox);
        const kept = onCanvas
            .filter((p) => bounds.some((r) => intersects(p.bbox!, r)))
            .map((p) => p.markup)
            .join('');
        const patches = layer.name === 'ground' ? groundPatches : '';
        // clip 放外层、transform 打在内层：窗口固定在身体坐标里，部件在窗口内动。
        return `<g clip-path="url(#${clipId(layer.name)})"><g data-part="${layer.name}">${kept}${patches}</g></g>`;
    };

    const ground = layerMarkup(LAYERS[0]);
    const figure = LAYERS.filter((l) => l.inFigure).map(layerMarkup).join('');
    const rest = LAYERS.filter((l) => !l.inFigure && l.name !== 'ground').map(layerMarkup).join('');

    return (
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1536" width="100%" height="100%">` +
        `<defs>${defs}</defs>${ground}<g data-part="figure">${figure}</g>${rest}</svg>`
    );
}
