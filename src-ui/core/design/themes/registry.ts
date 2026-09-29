// 主题注册表：主题 id、明暗、分组和选择器预览色的单一来源。
//
// 三类主题：
//   基础        auto / light / dark，tokens.css 手调，暖色画布 + 角落柔光
//   Catppuccin  latte / frappe / macchiato / mocha，tokens.css 手调
//   社区主题    palettes.ts 的调色板，paletteCss.ts 展开成 token
// 后两类都是第三方调色板，统一走纯色平面（<html data-theme-flat>），不铺暖色渐变。

import { PALETTE_THEMES, type PaletteThemeId, type ThemeScheme } from './palettes';

export type { ThemeScheme } from './palettes';

type CatppuccinTheme = 'latte' | 'frappe' | 'macchiato' | 'mocha';
export type ThemeMode = 'auto' | 'light' | 'dark' | CatppuccinTheme | PaletteThemeId;

/** 选择器卡片的缩略预览色。 */
export interface ThemePreview {
    value: ThemeMode;
    /** 卡片上的短名（组名已经写了族名） */
    label: string;
    /** 触发按钮上的全名 */
    fullLabel: string;
    canvas: string;
    sidebar: string;
    text: string;
    subtext: string;
    brand: string;
    accent: string;
}

export interface ThemeGroup {
    label: string;
    items: readonly ThemePreview[];
}

interface HandTunedTheme {
    preview: ThemePreview;
    /** null = 跟随系统 */
    scheme: ThemeScheme | null;
    flat: boolean;
}

const HAND_TUNED: readonly HandTunedTheme[] = [
    {
        scheme: null, flat: false,
        preview: {
            value: 'auto', label: '系统', fullLabel: '跟随系统',
            canvas: '#faf7f2', sidebar: '#ffe3ee',
            text: '#2c1f18', subtext: '#8a7d76', brand: '#ff6b3d', accent: '#f58fb6',
        },
    },
    {
        scheme: 'light', flat: false,
        preview: {
            value: 'light', label: '浅色', fullLabel: '浅色',
            canvas: '#faf7f2', sidebar: '#ffe3ee',
            text: '#2c1f18', subtext: '#8a7d76', brand: '#ff6b3d', accent: '#f58fb6',
        },
    },
    {
        scheme: 'dark', flat: false,
        preview: {
            value: 'dark', label: '暗色', fullLabel: '暗色',
            canvas: '#211f1d', sidebar: '#292725',
            text: '#f5f1ed', subtext: '#9e9890', brand: '#ff8a57', accent: '#f58fb6',
        },
    },
    {
        scheme: 'light', flat: true,
        preview: {
            value: 'latte', label: 'Latte', fullLabel: 'Catppuccin Latte',
            canvas: '#eff1f5', sidebar: '#e6e9ef',
            text: '#4c4f69', subtext: '#6c6f85', brand: '#8839ef', accent: '#1e66f5',
        },
    },
    {
        scheme: 'dark', flat: true,
        preview: {
            value: 'frappe', label: 'Frappé', fullLabel: 'Catppuccin Frappé',
            canvas: '#303446', sidebar: '#292c3c',
            text: '#c6d0f5', subtext: '#949cbb', brand: '#ca9ee6', accent: '#8caaee',
        },
    },
    {
        scheme: 'dark', flat: true,
        preview: {
            value: 'macchiato', label: 'Macchiato', fullLabel: 'Catppuccin Macchiato',
            canvas: '#24273a', sidebar: '#1e2030',
            text: '#cad3f5', subtext: '#939ab7', brand: '#c6a0f6', accent: '#8aadf4',
        },
    },
    {
        scheme: 'dark', flat: true,
        preview: {
            value: 'mocha', label: 'Mocha', fullLabel: 'Catppuccin Mocha',
            canvas: '#1e1e2e', sidebar: '#181825',
            text: '#cdd6f4', subtext: '#9399b2', brand: '#cba6f7', accent: '#89b4fa',
        },
    },
];

const handTuned = (ids: readonly ThemeMode[]) =>
    HAND_TUNED.filter((t) => ids.includes(t.preview.value)).map((t) => t.preview);

function paletteGroups(): ThemeGroup[] {
    const groups = new Map<string, ThemePreview[]>();
    for (const p of PALETTE_THEMES) {
        const items = groups.get(p.family) ?? [];
        items.push({
            value: p.id,
            label: p.label,
            fullLabel: p.name,
            canvas: p.canvas,
            sidebar: p.sidebar,
            text: p.text,
            subtext: p.text3,
            brand: p.brand,
            accent: p.accent,
        });
        groups.set(p.family, items);
    }
    return [...groups].map(([label, items]) => ({ label, items }));
}

/** 选择器分组，顺序即展示顺序。社区主题按 palettes.ts 里的族顺序。 */
export const THEME_GROUPS: readonly ThemeGroup[] = [
    { label: '基础', items: handTuned(['auto', 'light', 'dark']) },
    { label: 'Catppuccin', items: handTuned(['latte', 'frappe', 'macchiato', 'mocha']) },
    ...paletteGroups(),
];

const ALL_PREVIEWS = new Map<string, ThemePreview>(
    THEME_GROUPS.flatMap((g) => g.items.map((it) => [it.value, it] as const)),
);

export function findThemePreview(value: ThemeMode): ThemePreview | undefined {
    return ALL_PREVIEWS.get(value);
}

export function normalizeTheme(raw: unknown): ThemeMode {
    return typeof raw === 'string' && ALL_PREVIEWS.has(raw) ? (raw as ThemeMode) : 'auto';
}

const SCHEME = new Map<string, { scheme: ThemeScheme | null; flat: boolean }>([
    ...HAND_TUNED.map((t) => [t.preview.value, { scheme: t.scheme, flat: t.flat }] as const),
    ...PALETTE_THEMES.map((p) => [p.id, { scheme: p.scheme, flat: true }] as const),
]);

/** 主题的明暗；auto 返回 null（由系统 prefers-color-scheme 决定）。 */
export function themeScheme(theme: ThemeMode): ThemeScheme | null {
    return SCHEME.get(theme)?.scheme ?? null;
}

/** 第三方调色板主题：纯色平面，不铺暖色柔光 / 天幕渐变 / 启动极光。 */
export function isFlatTheme(theme: ThemeMode): boolean {
    return SCHEME.get(theme)?.flat ?? false;
}
