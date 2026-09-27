// 终端配色：背景 / 前景 / 光标跟界面主题的 token 走，16 色按明暗各给一套偏暖的色板。
// xterm 只认字面颜色，token 由调用方读成 hex 传进来。
// 浅色背景上 xterm 另外按 minimumContrastRatio 把太淡的前景色拉深，这里不用再为每个主题调。

export interface TerminalPalette {
    background: string;
    foreground: string;
    cursor: string;
    cursorAccent: string;
    selectionBackground: string;
    selectionInactiveBackground: string;
    scrollbarSliderBackground: string;
    scrollbarSliderHoverBackground: string;
    scrollbarSliderActiveBackground: string;
    overviewRulerBorder: string;
    black: string;
    red: string;
    green: string;
    yellow: string;
    blue: string;
    magenta: string;
    cyan: string;
    white: string;
    brightBlack: string;
    brightRed: string;
    brightGreen: string;
    brightYellow: string;
    brightBlue: string;
    brightMagenta: string;
    brightCyan: string;
    brightWhite: string;
}

type Ansi16 = Pick<
    TerminalPalette,
    | 'black' | 'red' | 'green' | 'yellow' | 'blue' | 'magenta' | 'cyan' | 'white'
    | 'brightBlack' | 'brightRed' | 'brightGreen' | 'brightYellow' | 'brightBlue'
    | 'brightMagenta' | 'brightCyan' | 'brightWhite'
>;

const DARK_ANSI: Ansi16 = {
    black: '#3a3431',
    red: '#f0716b',
    green: '#7ccf8f',
    yellow: '#e8c26a',
    blue: '#6fb3ff',
    magenta: '#d98ad8',
    cyan: '#5fcfcf',
    white: '#d9d2c9',
    brightBlack: '#8a817a',
    brightRed: '#ff8f87',
    brightGreen: '#9be3a9',
    brightYellow: '#f5d68a',
    brightBlue: '#91c6ff',
    brightMagenta: '#e9a8e8',
    brightCyan: '#86e2e2',
    brightWhite: '#f7f2ec',
};

const LIGHT_ANSI: Ansi16 = {
    black: '#2c1f18',
    red: '#c8373a',
    green: '#2f8a4f',
    yellow: '#9a6400',
    blue: '#1f6fc8',
    magenta: '#a33ea1',
    cyan: '#12848a',
    white: '#8a7f75',
    brightBlack: '#6f655d',
    brightRed: '#e04b47',
    brightGreen: '#3ba563',
    brightYellow: '#b07c00',
    brightBlue: '#3a86de',
    brightMagenta: '#bf4fbd',
    brightCyan: '#1b9aa1',
    brightWhite: '#a39990',
};

/** 「始终深色」时用的底色，和界面深色主题的内嵌底色同一个调子 */
export const FIXED_DARK = { background: '#1d1916', foreground: '#e9e2d8' };

export interface PaletteInput {
    background: string;
    foreground: string;
    accent: string;
}

function hexToRgb(hex: string): [number, number, number] | null {
    const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
    if (!m) return null;
    const n = Number.parseInt(m[1] as string, 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** 相对亮度（sRGB），0 最暗 1 最亮 */
export function luminance(hex: string): number {
    const rgb = hexToRgb(hex);
    if (!rgb) return 0;
    const [r, g, b] = rgb.map((v) => {
        const c = v / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    }) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function isDarkColor(hex: string): boolean {
    return luminance(hex) < 0.35;
}

/** `#rrggbb` 加上透明度，得到 `#rrggbbaa` */
export function withAlpha(hex: string, alpha: number): string {
    const rgb = hexToRgb(hex);
    if (!rgb) return hex;
    const a = Math.round(Math.max(0, Math.min(1, alpha)) * 255)
        .toString(16)
        .padStart(2, '0');
    return `#${rgb.map((v) => v.toString(16).padStart(2, '0')).join('')}${a}`;
}

export function buildPalette(input: PaletteInput, scheme: 'auto' | 'dark'): TerminalPalette {
    const base = scheme === 'dark' ? FIXED_DARK : { background: input.background, foreground: input.foreground };
    const dark = isDarkColor(base.background);
    const ansi = dark ? DARK_ANSI : LIGHT_ANSI;
    return {
        ...ansi,
        background: base.background,
        foreground: base.foreground,
        cursor: input.accent,
        cursorAccent: base.background,
        selectionBackground: withAlpha(input.accent, dark ? 0.35 : 0.3),
        selectionInactiveBackground: withAlpha(input.accent, dark ? 0.18 : 0.15),
        scrollbarSliderBackground: withAlpha(base.foreground, 0.14),
        scrollbarSliderHoverBackground: withAlpha(base.foreground, 0.26),
        scrollbarSliderActiveBackground: withAlpha(base.foreground, 0.36),
        overviewRulerBorder: withAlpha(base.foreground, 0.08),
    };
}
