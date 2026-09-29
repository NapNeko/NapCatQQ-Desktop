// 社区调色板主题 → 整套 design token。
//
// tokens.css 里的内置主题和 Catppuccin 是逐个手调的整块覆写；社区主题数量多，
// 每个只给一份紧凑调色板（palettes.ts），在这里按明 / 暗两套公式展开成同样的
// token 集合，启动时一次性注入 <style>。公式照 tokens.css 里默认明 / 暗块的
// 比例来，组件侧看到的变量和内置主题完全一致。

import type { ThemePalette } from './palettes';

type Rgb = readonly [number, number, number];

function hexToRgb(hex: string): Rgb {
    const h = hex.replace('#', '');
    const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h.slice(0, 6);
    return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as unknown as Rgb;
}

function rgba(hex: string, alpha: number): string {
    const [r, g, b] = hexToRgb(hex);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function relativeLuminance(hex: string): number {
    const [r, g, b] = hexToRgb(hex).map((v) => {
        const c = v / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
    const la = relativeLuminance(a);
    const lb = relativeLuminance(b);
    return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** 品牌色按钮上的字：白字和主题最深色里挑对比度高的那个。暗色主题的品牌色多是粉彩，白字读不清。 */
export function pickOnColor(fill: string, darkInk: string): string {
    return contrastRatio(fill, '#ffffff') >= contrastRatio(fill, darkInk) ? '#ffffff' : darkInk;
}

const mix = (a: string, pct: number, b: string) => `color-mix(in srgb, ${a} ${pct}%, ${b})`;

/** 一个颜色展开成 50~900 阶梯：浅阶混白、深阶混黑，明暗主题同一套，和 Catppuccin 块的阶梯走向一致。 */
function ramp(prefix: string, base: string): string[] {
    const lighter: Array<[number, number]> = [[50, 8], [100, 16], [200, 32], [300, 55], [400, 78]];
    const darker: Array<[number, number]> = [[600, 86], [700, 72], [800, 58], [900, 44]];
    return [
        ...lighter.map(([step, pct]) => `--${prefix}-${step}: ${mix(base, pct, '#ffffff')};`),
        `--${prefix}-500: ${base};`,
        ...darker.map(([step, pct]) => `--${prefix}-${step}: ${mix(base, pct, '#000000')};`),
    ];
}

function stateDecls(p: ThemePalette): string[] {
    const dark = (c: string) => mix(c, 86, '#000000');
    return [
        `--green-500: ${p.success};`,
        `--green-600: ${dark(p.success)};`,
        `--amber-500: ${p.warning};`,
        `--amber-600: ${dark(p.warning)};`,
        `--rose-500: ${p.danger};`,
        `--rose-600: ${dark(p.danger)};`,
        `--blue-500: ${p.info};`,
        `--blue-600: ${dark(p.info)};`,
        `--purple-500: ${p.accent};`,
        '--state-success: var(--green-500);',
        '--state-warning: var(--amber-500);',
        '--state-danger: var(--rose-500);',
        '--state-info: var(--blue-500);',
        `--state-neutral: ${p.text3};`,
    ];
}

function lightDecls(p: ThemePalette, inset: string): string[] {
    const ink = p.text;
    return [
        '--surface-field: var(--surface-inset);',
        '--surface-hero: color-mix(in srgb, var(--surface-canvas) 42%, var(--brand-50) 58%);',
        '--log-ansi-l-min: 0.36;',
        '--log-ansi-l-max: 0.52;',
        `--border-subtle: ${rgba(ink, 0.07)};`,
        `--border-default: ${rgba(ink, 0.13)};`,
        `--border-strong: ${rgba(ink, 0.22)};`,
        '--brand-soft: color-mix(in srgb, var(--brand-500) 14%, var(--surface-card));',
        '--brand-tint: color-mix(in srgb, var(--brand-500) 7%, var(--surface-card));',
        '--accent-soft: color-mix(in srgb, var(--accent-500) 14%, var(--surface-card));',
        '--accent-tint: color-mix(in srgb, var(--accent-500) 7%, var(--surface-card));',
        '--state-success-bg: color-mix(in srgb, var(--green-500) 12%, transparent);',
        '--state-warning-bg: color-mix(in srgb, var(--amber-500) 14%, transparent);',
        '--state-danger-bg: color-mix(in srgb, var(--rose-500) 12%, transparent);',
        '--state-info-bg: color-mix(in srgb, var(--blue-500) 12%, transparent);',
        `--shadow-sm: 0 1px 2px ${rgba(ink, 0.05)};`,
        `--shadow-md: 0 4px 12px ${rgba(ink, 0.07)}, 0 1px 2px ${rgba(ink, 0.04)};`,
        `--shadow-lg: 0 12px 32px ${rgba(ink, 0.09)}, 0 2px 6px ${rgba(ink, 0.05)};`,
        '--shadow-glow-success: 0 0 12px color-mix(in srgb, var(--green-500) 35%, transparent);',
        '--shadow-glow-brand: 0 0 16px color-mix(in srgb, var(--brand-500) 30%, transparent);',
        `--scrollbar-thumb-rest: ${rgba(ink, 0.12)};`,
        `--scrollbar-thumb-hover: ${rgba(ink, 0.24)};`,
        `--scrollbar-thumb-active: ${rgba(ink, 0.36)};`,
        `--qr-foreground: ${p.text};`,
        `--qr-background: ${inset};`,
        '--infobar-info-bg: var(--surface-inset);',
        '--infobar-success-bg: color-mix(in srgb, var(--green-500) 16%, var(--surface-card));',
        '--infobar-warning-bg: color-mix(in srgb, var(--amber-500) 22%, var(--surface-card));',
        '--infobar-danger-bg: color-mix(in srgb, var(--rose-500) 14%, var(--surface-card));',
        '--infobar-info-border: var(--border-default);',
        '--infobar-success-border: color-mix(in srgb, var(--green-600) 28%, var(--border-default));',
        '--infobar-warning-border: color-mix(in srgb, var(--amber-600) 32%, var(--border-default));',
        '--infobar-danger-border: color-mix(in srgb, var(--rose-600) 30%, var(--border-default));',
        '--infobar-icon-well-info: color-mix(in srgb, var(--blue-500) 18%, var(--surface-inset));',
        '--infobar-icon-well-success: color-mix(in srgb, var(--green-500) 22%, var(--surface-card));',
        '--infobar-icon-well-warning: color-mix(in srgb, var(--amber-500) 26%, var(--surface-card));',
        '--infobar-icon-well-danger: color-mix(in srgb, var(--rose-500) 20%, var(--surface-card));',
    ];
}

function darkDecls(p: ThemePalette, inset: string): string[] {
    const ink = p.text;
    return [
        '--surface-field: var(--surface-inset);',
        '--surface-hero: color-mix(in srgb, var(--surface-elevated) 88%, var(--brand-500) 12%);',
        '--log-ansi-l-min: 0.72;',
        '--log-ansi-l-max: 0.9;',
        `--border-subtle: ${rgba(ink, 0.08)};`,
        `--border-default: ${rgba(ink, 0.15)};`,
        `--border-strong: ${rgba(ink, 0.25)};`,
        '--brand-soft: color-mix(in srgb, var(--brand-500) 16%, var(--surface-card));',
        '--brand-tint: color-mix(in srgb, var(--brand-500) 10%, transparent);',
        '--accent-soft: color-mix(in srgb, var(--accent-500) 16%, var(--surface-card));',
        '--accent-tint: color-mix(in srgb, var(--accent-500) 10%, transparent);',
        '--state-success-bg: color-mix(in srgb, var(--green-500) 14%, transparent);',
        '--state-warning-bg: color-mix(in srgb, var(--amber-500) 14%, transparent);',
        '--state-danger-bg: color-mix(in srgb, var(--rose-500) 14%, transparent);',
        '--state-info-bg: color-mix(in srgb, var(--blue-500) 14%, transparent);',
        `--shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.35), 0 0 0 1px ${rgba(ink, 0.05)};`,
        `--shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4), 0 0 0 1px ${rgba(ink, 0.07)};`,
        `--shadow-lg: 0 12px 32px rgba(0, 0, 0, 0.5), 0 0 0 1px ${rgba(ink, 0.09)};`,
        '--shadow-glow-success: 0 0 12px color-mix(in srgb, var(--green-500) 25%, transparent);',
        '--shadow-glow-brand: 0 0 16px color-mix(in srgb, var(--brand-500) 20%, transparent);',
        `--scrollbar-thumb-rest: ${rgba(ink, 0.12)};`,
        `--scrollbar-thumb-hover: ${rgba(ink, 0.24)};`,
        `--scrollbar-thumb-active: ${rgba(ink, 0.36)};`,
        // 深底上的二维码画成浅底深码：扫码端对反色码的支持参差不齐。
        `--qr-foreground: ${inset};`,
        `--qr-background: ${p.text};`,
        '--infobar-info-bg: var(--surface-elevated);',
        '--infobar-success-bg: color-mix(in srgb, var(--green-500) 24%, var(--surface-card));',
        '--infobar-warning-bg: color-mix(in srgb, var(--amber-500) 26%, var(--surface-card));',
        '--infobar-danger-bg: color-mix(in srgb, var(--rose-500) 22%, var(--surface-card));',
        '--infobar-info-border: var(--border-default);',
        '--infobar-success-border: color-mix(in srgb, var(--green-500) 35%, var(--border-subtle));',
        '--infobar-warning-border: color-mix(in srgb, var(--amber-500) 38%, var(--border-subtle));',
        '--infobar-danger-border: color-mix(in srgb, var(--rose-500) 36%, var(--border-subtle));',
        '--infobar-icon-well-info: color-mix(in srgb, var(--blue-500) 28%, var(--surface-inset));',
        '--infobar-icon-well-success: color-mix(in srgb, var(--green-500) 32%, var(--surface-inset));',
        '--infobar-icon-well-warning: color-mix(in srgb, var(--amber-500) 34%, var(--surface-inset));',
        '--infobar-icon-well-danger: color-mix(in srgb, var(--rose-500) 30%, var(--surface-inset));',
    ];
}

/**
 * 一份调色板展开成一个 CSS 块。
 *
 * 选择器用 `html:root[...]`：比 tokens.css 里 `@media (prefers-color-scheme: dark) :root:not([data-theme="light"])`
 * 高一级，系统是暗色时浅色社区主题也不会被暗色块盖掉，不依赖 <style> 和样式表谁先插进 head。
 */
export function buildPaletteThemeCss(p: ThemePalette): string {
    const darkInk = p.scheme === 'dark' ? p.inset : p.text;
    const decls = [
        `--surface-canvas: ${p.canvas};`,
        `--surface-card: ${p.card};`,
        `--surface-elevated: ${p.elevated};`,
        `--surface-inset: ${p.inset};`,
        `--surface-muted: ${p.muted};`,
        `--surface-sidebar: ${p.sidebar};`,
        `--text-primary: ${p.text};`,
        `--text-secondary: ${p.text2};`,
        `--text-tertiary: ${p.text3};`,
        `--text-disabled: ${p.text4};`,
        `--text-on-brand: ${pickOnColor(p.brand, darkInk)};`,
        `--text-on-accent: ${pickOnColor(p.accent, darkInk)};`,
        `--text-hero-title: ${p.brand};`,
        `--text-hero-accent: ${p.accent};`,
        ...ramp('brand', p.brand),
        ...ramp('accent', p.accent),
        '--action-primary: var(--brand-500);',
        '--action-primary-hover: var(--brand-600);',
        '--action-primary-active: var(--brand-700);',
        '--action-secondary: var(--accent-500);',
        '--action-secondary-hover: var(--accent-600);',
        '--border-focus: var(--brand-500);',
        ...stateDecls(p),
        ...(p.scheme === 'dark' ? darkDecls(p, p.inset) : lightDecls(p, p.inset)),
    ];
    return `html:root[data-theme="${p.id}"] {\n${decls.map((d) => `   ${d}`).join('\n')}\n}`;
}

const STYLE_ID = 'ncd-palette-themes';

/** 把全部社区主题的 token 块注入一次；重复调用不重复插。 */
export function installPaletteThemeStyles(palettes: readonly ThemePalette[]): void {
    if (typeof document === 'undefined' || document.getElementById(STYLE_ID)) return;
    const style = document.createElement('style');
    style.id = STYLE_ID;
    style.textContent = palettes.map(buildPaletteThemeCss).join('\n\n');
    document.head.appendChild(style);
}
