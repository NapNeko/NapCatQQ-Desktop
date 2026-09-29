// 与 tokens --surface-canvas 对齐，供首屏 / WebView 底色同步（避免暗色主题闪白）。

const CANVAS_FALLBACK_LIGHT = '#faf7f2';

export function readSurfaceCanvasColor(): string {
    if (typeof document === 'undefined') return CANVAS_FALLBACK_LIGHT;
    const v = getComputedStyle(document.documentElement)
        .getPropertyValue('--surface-canvas')
        .trim();
    return v || CANVAS_FALLBACK_LIGHT;
}

/** 把当前主题画布色写到 html/body/#root，与 Tauri 窗口透明区一致。 */
export function syncRootChromeBackground(): void {
    if (typeof document === 'undefined') return;
    const bg = readSurfaceCanvasColor();
    const html = document.documentElement;
    html.style.backgroundColor = bg;
    document.body.style.backgroundColor = bg;
    const root = document.getElementById('root');
    if (root) root.style.backgroundColor = bg;
}

/** 当前是否为偏暗画布（用于 Splash / 主题过渡里禁用提亮）。 */
export function isDarkSurfaceCanvas(): boolean {
    const bg = readSurfaceCanvasColor();
    if (!bg.startsWith('#') || bg.length < 7) {
        return typeof window !== 'undefined' &&
            window.matchMedia('(prefers-color-scheme: dark)').matches;
    }
    const hex = bg.slice(1);
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    return lum < 0.45;
}
