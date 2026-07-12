// design token 的 TS 镜像。仅给必须在 JS 端取值的场景使用：
//   - 图表 / canvas / QR 等不能走 CSS variable 的 prop
//   - 计算派生值（CSS color-mix 不够用时）
//   - Catppuccin flavor 语义色（品牌色 / 状态色等）
// 普通组件应该走 className + tailwind utility / 或直接 var(--xxx)，
// 不要从这里 import 字面值。动画 token 在 core/design/motion.ts。

import {
    catppuccinFlavors,
    type CatppuccinFlavorName,
} from './catppuccin';

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

// Catppuccin 四味语义色块。给 JS 端需要某 flavor 下 brand / text 字面色的场景
// （图表路径、QR 前景等）。结构对齐 tokens 主块的语义层。
export const catppuccinSemantic: Record<
    CatppuccinFlavorName,
    {
        brand500: string; accent500: string; success: string; warning: string; danger: string; info: string;
        textPrimary: string; canvas: string; card: string; textOnBrand: string
    }
> = {
    latte: {
        brand500: catppuccinFlavors.latte.mauve,    // #8839ef
        accent500: catppuccinFlavors.latte.pink,    // #ea76cb
        success: catppuccinFlavors.latte.green,     // #40a02b
        warning: catppuccinFlavors.latte.yellow,    // #df8e1d
        danger: catppuccinFlavors.latte.red,        // #d20f39
        info: catppuccinFlavors.latte.blue,         // #1e66f5
        textPrimary: catppuccinFlavors.latte.text,  // #4c4f69
        canvas: catppuccinFlavors.latte.base,       // #eff1f5
        card: catppuccinFlavors.latte.mantle,       // #e6e9ef
        textOnBrand: '#ffffff',
    },
    frappe: {
        brand500: catppuccinFlavors.frappe.mauve,    // #ca9ee6
        accent500: catppuccinFlavors.frappe.pink,    // #f4b8e4
        success: catppuccinFlavors.frappe.green,     // #a6d189
        warning: catppuccinFlavors.frappe.yellow,    // #e5c890
        danger: catppuccinFlavors.frappe.red,        // #e78284
        info: catppuccinFlavors.frappe.blue,         // #8caaee
        textPrimary: catppuccinFlavors.frappe.text,  // #c6d0f5
        canvas: catppuccinFlavors.frappe.base,       // #303446
        card: catppuccinFlavors.frappe.mantle,       // #292c3c
        textOnBrand: '#ffffff',
    },
    macchiato: {
        brand500: catppuccinFlavors.macchiato.mauve,   // #c6a0f6
        accent500: catppuccinFlavors.macchiato.pink,    // #f5bde6
        success: catppuccinFlavors.macchiato.green,     // #a6da95
        warning: catppuccinFlavors.macchiato.yellow,    // #eed49f
        danger: catppuccinFlavors.macchiato.red,        // #ed8796
        info: catppuccinFlavors.macchiato.blue,         // #8aadf4
        textPrimary: catppuccinFlavors.macchiato.text,  // #cad3f5
        canvas: catppuccinFlavors.macchiato.base,       // #24273a
        card: catppuccinFlavors.macchiato.mantle,       // #1e2030
        textOnBrand: '#ffffff',
    },
    mocha: {
        brand500: catppuccinFlavors.mocha.mauve,   // #cba6f7
        accent500: catppuccinFlavors.mocha.pink,    // #f5c2e7
        success: catppuccinFlavors.mocha.green,     // #a6e3a1
        warning: catppuccinFlavors.mocha.yellow,    // #f9e2af
        danger: catppuccinFlavors.mocha.red,        // #f38ba8
        info: catppuccinFlavors.mocha.blue,         // #89b4fa
        textPrimary: catppuccinFlavors.mocha.text,  // #cdd6f4
        canvas: catppuccinFlavors.mocha.base,       // #1e1e2e
        card: catppuccinFlavors.mocha.mantle,       // #181825
        textOnBrand: '#ffffff',
    },
} as const;

// 完整 flavor 原始色值（需要全部 26 色时）。
export { catppuccinFlavors } from './catppuccin';
