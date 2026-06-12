// 圆角体系：三档全局风格（方正 / 标准 / 圆润）。
//
// 设计思路:
//   tokens.css 里定义了基准阶梯（xs 4 / sm 8 / md 12 / lg 20 / xl 28 / pill 9999），
//   组件通过 className="rounded-md" 引用。本模块在三档风格间按统一系数缩放所有阶梯，
//   保持"小标签 < 卡片 < 大卡 < Hero 卡"的层级比例不变，pill 始终 9999px。
//
//   切档时只需在 :root 上覆盖 --radius-* 的值，组件零改动。
//
// 三档系数:
//   square  0.5×  →  2 / 4 / 6 / 10 / 14 px   克制方正
//   standard 1.0× →  4 / 8 / 12 / 20 / 28 px   默认平衡
//   round   1.5×  →  6 / 12 / 18 / 30 / 42 px  饱满圆润
//
// pill 9999px 不受系数影响——状态指示灯、进度条等需要完全圆形的控件保持不变。

/** 三档圆角风格。 */
export type RadiusStyle = 'square' | 'standard' | 'round';

/** 默认风格。 */
export const RADIUS_STYLE_DEFAULT: RadiusStyle = 'standard';

/** 基准值（px），对应 standard 档。与 tokens.css 保持一致。 */
export const RADIUS_BASE = {
    xs: 4,
    sm: 8,
    md: 12,
    lg: 20,
    xl: 28,
} as const;

/** pill 不受系数影响。 */
export const RADIUS_PILL = 9999;

/** 三档系数。 */
export const RADIUS_SCALE: Record<RadiusStyle, number> = {
    square: 0.5,
    standard: 1.0,
    round: 1.5,
};

/** 每档的中文标签，供 UI 显示。 */
export const RADIUS_LABELS: Record<RadiusStyle, string> = {
    square: '方正',
    standard: '标准',
    round: '圆润',
};

/** 校验未知值，兜底 standard。 */
export function normalizeRadiusStyle(raw: unknown): RadiusStyle {
    return raw === 'square' || raw === 'round' ? raw : 'standard';
}

/**
 * 把指定档位的圆角值写入 :root 的 CSS 自定义属性。
 * 组件无需任何改动——Tailwind 的 rounded-* utilities 通过 @theme 间接引用这些变量。
 *
 * AppNext 启动时 + 设置页保存时各调一次。
 */
export function applyRadiusStyle(style: RadiusStyle): void {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    const scale = RADIUS_SCALE[style];

    for (const [key, base] of Object.entries(RADIUS_BASE)) {
        const value = Math.round(base * scale);
        root.style.setProperty(`--radius-${key}`, `${value}px`);
    }
    // pill 永远不变
    root.style.setProperty('--radius-pill', `${RADIUS_PILL}px`);
}
