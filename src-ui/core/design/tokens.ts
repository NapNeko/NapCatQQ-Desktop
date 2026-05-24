// design token 的 TS 镜像。仅给"必须在 JS 端取值"的场景使用：
//   - recharts 折线颜色（不能用 CSS variable 走 prop）
//   - framer-motion transition 数值
//   - 计算派生值（CSS color-mix 不够用时）
// 普通组件应该走 className + tailwind utility / 或直接 var(--xxx)，
// 不要从这里 import 字面值。

export const tokens = {
    brand: {
        50: '#fff5ec',
        100: '#ffe9cf',
        300: '#ffb586',
        500: '#ff6b3d',
        600: '#e85220',
        700: '#c33f15',
    },
    accent: {
        100: '#ffe3ee',
        300: '#fbc4dc',
        500: '#f58fb6',
        700: '#c76a8e',
    },
    neutral: {
        0: '#ffffff',
        50: '#faf7f2',
        100: '#f4efe7',
        200: '#e9e2d5',
        400: '#b8ad9b',
        500: '#9a8e84',
        700: '#5c534b',
        900: '#2c1f18',
    },
    state: {
        success: '#4fb477',
        warning: '#f2b43a',
        danger: '#e85b57',
        info: '#5eb1ff',
    },
    motion: {
        fast: 0.12,
        base: 0.18,
        slow: 0.28,
        ease: [0.16, 1, 0.3, 1] as const,
    },
} as const;

export type Tokens = typeof tokens;
