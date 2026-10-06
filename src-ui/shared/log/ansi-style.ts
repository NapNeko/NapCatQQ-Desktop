// 终端颜色落到日志面板。上游是照黑底终端配的色，浅色主题下直接用大多看不清：
// 有彩色的保留色相和彩度，亮度夹进当前主题读得清的区间（tokens.css 的 --log-ansi-l-min / max）；
// 灰、白、黑这类没彩度的换成主题正文色，白是强调、灰是弱化，和在终端里的意思一样。

import type { CSSProperties } from 'react';
import type { AnsiColor, AnsiStyle } from '../../core/domain/events/ansi';

type Rgb = readonly [number, number, number];

// 16 色用 VS Code 终端那套，比 xterm 默认的柔和，色相也好认
const BASE16: readonly Rgb[] = [
    [0, 0, 0],
    [205, 49, 49],
    [13, 188, 121],
    [229, 229, 16],
    [36, 114, 200],
    [188, 63, 188],
    [17, 168, 205],
    [229, 229, 229],
    [102, 102, 102],
    [241, 76, 76],
    [35, 209, 139],
    [245, 245, 67],
    [59, 142, 234],
    [214, 112, 214],
    [41, 184, 219],
    [255, 255, 255],
];

/** 调色板序号换成 RGB：16–231 是 6×6×6 立方，232–255 是灰阶 */
export function ansiRgb(color: AnsiColor): Rgb {
    if (color.kind === 'rgb') return [color.r, color.g, color.b];
    const i = color.index;
    if (i < 16) return BASE16[i];
    if (i < 232) {
        const n = i - 16;
        const level = (v: number) => (v === 0 ? 0 : 55 + v * 40);
        return [level(Math.floor(n / 36)), level(Math.floor(n / 6) % 6), level(n % 6)];
    }
    const gray = 8 + (i - 232) * 10;
    return [gray, gray, gray];
}

function toLinear(v: number): number {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** sRGB → OKLCH（Björn Ottosson 的矩阵），h 用角度 */
export function rgbToOklch(r: number, g: number, b: number): { l: number; c: number; h: number } {
    const lr = toLinear(r);
    const lg = toLinear(g);
    const lb = toLinear(b);
    const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
    const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
    const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
    const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
    const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
    const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
    const h = (Math.atan2(B, A) * 180) / Math.PI;
    return { l: L, c: Math.hypot(A, B), h: h < 0 ? h + 360 : h };
}

// 一份日志里来来回回就那十几种颜色
const cssByColor = new Map<string, string>();

export function ansiColorCss(color: AnsiColor): string {
    const key = color.kind === 'rgb' ? `${color.r},${color.g},${color.b}` : `p${color.index}`;
    const hit = cssByColor.get(key);
    if (hit) return hit;
    const [r, g, b] = ansiRgb(color);
    const { l, c, h } = rgbToOklch(r, g, b);
    let css: string;
    if (c < 0.04) {
        css =
            l >= 0.8
                ? 'var(--text-primary)'
                : l >= 0.6
                  ? 'var(--text-secondary)'
                  : 'var(--text-tertiary)';
    } else {
        const lightness = `clamp(var(--log-ansi-l-min, 0.36), ${l.toFixed(3)}, var(--log-ansi-l-max, 0.52))`;
        css = `oklch(${lightness} ${c.toFixed(3)} ${h.toFixed(1)})`;
    }
    cssByColor.set(key, css);
    return css;
}

export function ansiCss(style: AnsiStyle): CSSProperties {
    const css: CSSProperties = {};
    if (style.fg) css.color = ansiColorCss(style.fg);
    if (style.bold) css.fontWeight = 600;
    if (style.dim) css.opacity = 0.7;
    if (style.italic) css.fontStyle = 'italic';
    if (style.underline) css.textDecoration = 'underline';
    return css;
}
