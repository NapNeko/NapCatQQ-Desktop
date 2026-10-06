import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { PALETTE_THEMES } from './palettes';
import { buildPaletteThemeCss, contrastRatio, pickOnColor } from './paletteCss';
import { THEME_GROUPS, isFlatTheme, normalizeTheme, themeScheme } from './registry';

// vitest 里 CSS 的 ?raw 导入拿到的是空串，直接读文件。
const tokensCss = readFileSync(resolve(__dirname, '../tokens.css'), 'utf-8');

/** tokens.css 里某个选择器块的声明（变量名 → 原值）。 */
function declsOfBlock(css: string, selector: string): Map<string, string> {
    const start = css.indexOf(selector);
    const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start));
    return new Map(
        [...body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]),
    );
}

function varsOfBlock(css: string, selector: string): string[] {
    return [...declsOfBlock(css, selector).keys()];
}

/** 在主题块里解析一个变量，var() 先找本块再退到 :root，直到拿到 hex。 */
function resolveHex(selector: string, name: string): string {
    const own = declsOfBlock(tokensCss, selector);
    const root = declsOfBlock(tokensCss, ':root {');
    let value = own.get(name) ?? root.get(name) ?? '';
    for (let i = 0; i < 5 && value.startsWith('var('); i++) {
        const ref = value.slice(4, -1).trim();
        value = own.get(ref) ?? root.get(ref) ?? '';
    }
    if (!/^#[0-9a-f]{6}$/i.test(value))
        throw new Error(`${selector} ${name} 解析不出 hex：${value}`);
    return value;
}

describe('theme registry', () => {
    it('keeps every theme id unique across hand-tuned and palette themes', () => {
        const ids = THEME_GROUPS.flatMap((g) => g.items.map((it) => it.value));
        expect(new Set(ids).size).toBe(ids.length);
    });

    it('orders palette families so the picker rows pack into 4 columns', () => {
        const spans = THEME_GROUPS.slice(2).map((g) => g.items.length);
        let row = 0;
        for (const span of spans) {
            row += span;
            expect(row).toBeLessThanOrEqual(4);
            if (row === 4) row = 0;
        }
        expect(row).toBe(0);
    });

    it('falls back to auto for unknown or removed theme ids', () => {
        expect(normalizeTheme('tokyo-night-storm')).toBe('tokyo-night-storm');
        expect(normalizeTheme('mocha')).toBe('mocha');
        expect(normalizeTheme('solarized')).toBe('auto');
        expect(normalizeTheme(42)).toBe('auto');
    });

    it('reports scheme and flatness per theme', () => {
        expect(themeScheme('auto')).toBeNull();
        expect(themeScheme('latte')).toBe('light');
        expect(themeScheme('rose-pine-dawn')).toBe('light');
        expect(themeScheme('dracula')).toBe('dark');
        expect(isFlatTheme('dark')).toBe(false);
        expect(isFlatTheme('frappe')).toBe(true);
        expect(isFlatTheme('nord')).toBe(true);
    });
});

describe('palette theme css', () => {
    // 系统是暗色时 tokens.css 的暗色块也会命中；社区主题漏写哪个变量，浅色主题就会混进暗色值。
    const darkBlockVars = varsOfBlock(tokensCss, ':root[data-theme="dark"]');
    const lightRootVars = varsOfBlock(tokensCss, ':root[data-theme="latte"]');

    it('reads the reference blocks from tokens.css', () => {
        expect(darkBlockVars.length).toBeGreaterThan(40);
        expect(lightRootVars.length).toBeGreaterThan(40);
    });

    it.each(PALETTE_THEMES.map((p) => [p.id, p] as const))(
        '%s overrides every themed token',
        (_, p) => {
            const css = buildPaletteThemeCss(p);
            expect(css.startsWith(`html:root[data-theme="${p.id}"] {`)).toBe(true);
            for (const name of new Set([...darkBlockVars, ...lightRootVars])) {
                expect(css, name).toContain(`${name}:`);
            }
        },
    );

    it.each(PALETTE_THEMES.map((p) => [p.id, p] as const))(
        '%s keeps body text readable',
        (_, p) => {
            expect(contrastRatio(p.text, p.canvas)).toBeGreaterThanOrEqual(4.5);
            expect(contrastRatio(p.text, p.card)).toBeGreaterThanOrEqual(4.5);
            expect(contrastRatio(p.text2, p.canvas)).toBeGreaterThanOrEqual(3);
        },
    );

    // 实测（屏幕反光 + 轻微失焦的拍屏模拟）：1.4:1 的深底深码 WeChat 引擎只扫出一半，
    // 4:1 左右已经和高对比码一样稳。3.5 留一点余量。
    const QR_MIN_CONTRAST = 3.5;

    it.each([
        ':root {',
        '@media (prefers-color-scheme: dark)',
        ':root[data-theme="dark"]',
        ':root[data-theme="latte"]',
        ':root[data-theme="frappe"]',
        ':root[data-theme="macchiato"]',
        ':root[data-theme="mocha"]',
    ])('hand-tuned %s draws the QR code dark on light with enough contrast', (selector) => {
        const fg = resolveHex(selector, '--qr-foreground');
        const bg = resolveHex(selector, '--qr-background');
        expect(contrastRatio(fg, bg)).toBeGreaterThanOrEqual(QR_MIN_CONTRAST);
        expect(contrastRatio(fg, '#000000')).toBeLessThan(contrastRatio(bg, '#000000'));
    });

    it.each(PALETTE_THEMES.map((p) => [p.id, p] as const))(
        '%s draws the QR code dark on light with enough contrast',
        (_, p) => {
            const css = buildPaletteThemeCss(p);
            const fg = css.match(/--qr-foreground: (#[0-9a-f]{6});/i)![1];
            const bg = css.match(/--qr-background: (#[0-9a-f]{6});/i)![1];
            expect(contrastRatio(fg, bg)).toBeGreaterThanOrEqual(QR_MIN_CONTRAST);
            expect(contrastRatio(fg, '#000000')).toBeLessThan(contrastRatio(bg, '#000000'));
        },
    );

    it('picks dark text on pastel brand colors and white on saturated ones', () => {
        expect(pickOnColor('#c4a7e7', '#13111d')).toBe('#13111d');
        expect(pickOnColor('#0969da', '#1f2328')).toBe('#ffffff');
    });
});
