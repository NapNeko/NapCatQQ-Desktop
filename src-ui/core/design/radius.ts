// 圆角体系：三档全局风格（方正 / 标准 / 圆润）。

/** 三档圆角风格。 */
export type RadiusStyle = 'square' | 'standard' | 'round';

/** 默认风格。 */
export const RADIUS_STYLE_DEFAULT: RadiusStyle = 'standard';

type RadiusValues = Record<'xs' | 'sm' | 'md' | 'lg' | 'xl', number>;

// 独立定档避免小圆角缩放取整后过于接近；standard 与 tokens.css 保持一致。
export const RADIUS_PRESETS = {
    square: { xs: 0, sm: 1, md: 2, lg: 3, xl: 4 },
    standard: { xs: 2, sm: 4, md: 6, lg: 10, xl: 14 },
    round: { xs: 4, sm: 8, md: 12, lg: 20, xl: 28 },
} as const satisfies Record<RadiusStyle, RadiusValues>;

/** 圆形控件独立于圆角档位。 */
export const RADIUS_PILL = 9999;

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
    for (const [key, value] of Object.entries(RADIUS_PRESETS[style])) {
        root.style.setProperty(`--radius-${key}`, `${value}px`);
    }
    // pill 永远不变
    root.style.setProperty('--radius-pill', `${RADIUS_PILL}px`);
}
