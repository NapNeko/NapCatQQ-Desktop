import { describe, expect, it } from 'vitest';
import {
    FIXED_DARK,
    buildPalette,
    isDarkColor,
    luminance,
    withAlpha,
    type PaletteInput,
} from './palette';

const LIGHT_BG = '#faf7f2';
const DARK_BG = '#1d1916';
const ACCENT = '#ff6b3d';

const input = (background: string, foreground: string): PaletteInput => ({
    background,
    foreground,
    accent: ACCENT,
});

describe('luminance', () => {
    it('白 1、黑 0，灰在中间', () => {
        expect(luminance('#ffffff')).toBeCloseTo(1, 5);
        expect(luminance('#000000')).toBe(0);
        expect(luminance('#808080')).toBeGreaterThan(0.2);
        expect(luminance('#808080')).toBeLessThan(0.3);
    });

    it('同一色相亮度随明度单调上升', () => {
        expect(luminance('#ff0000')).toBeGreaterThan(luminance('#800000'));
        expect(luminance('#00ff00')).toBeGreaterThan(luminance('#008000'));
    });

    it('非法输入按最暗处理而不是 NaN', () => {
        expect(luminance('not-a-color')).toBe(0);
        expect(luminance('#12345')).toBe(0);
    });

    it('容忍 # 前缀缺失与首尾空白', () => {
        expect(luminance(' #FFFFFF ')).toBeCloseTo(1, 5);
        expect(luminance('ffffff')).toBeCloseTo(1, 5);
    });
});

describe('isDarkColor', () => {
    it('深底判暗、浅底判亮', () => {
        expect(isDarkColor(DARK_BG)).toBe(true);
        expect(isDarkColor(LIGHT_BG)).toBe(false);
        expect(isDarkColor('#000000')).toBe(true);
        expect(isDarkColor('#ffffff')).toBe(false);
    });

    it('阈值 0.35 两侧各取代表色', () => {
        // #767676 亮度约 0.18、#a8a8a8 约 0.39，跨在阈值两侧
        expect(isDarkColor('#767676')).toBe(true);
        expect(isDarkColor('#a8a8a8')).toBe(false);
    });
});

describe('withAlpha', () => {
    it('追加两位十六进制 alpha', () => {
        expect(withAlpha('#ff6b3d', 1)).toBe('#ff6b3dff');
        expect(withAlpha('#ff6b3d', 0)).toBe('#ff6b3d00');
        expect(withAlpha('#ff6b3d', 0.5)).toBe('#ff6b3d80');
    });

    it('alpha 越界夹到 [0,1]', () => {
        expect(withAlpha('#ff6b3d', 2)).toBe('#ff6b3dff');
        expect(withAlpha('#ff6b3d', -1)).toBe('#ff6b3d00');
    });

    it('非法颜色原样返回（交给 xterm 自己报错，不吞色）', () => {
        expect(withAlpha('rebeccapurple', 0.5)).toBe('rebeccapurple');
    });

    it('单字节通道补零', () => {
        expect(withAlpha('#000000', 0.05)).toBe('#0000000d');
    });
});

describe('buildPalette', () => {
    it('auto + 浅色背景：用输入底色、亮色 ANSI、光标用主题强调色', () => {
        const p = buildPalette(input(LIGHT_BG, '#2c1f18'), 'auto');
        expect(p.background).toBe(LIGHT_BG);
        expect(p.foreground).toBe('#2c1f18');
        expect(p.cursor).toBe(ACCENT);
        expect(p.cursorAccent).toBe(LIGHT_BG);
        // 浅底用偏深的 red（LIGHT_ANSI），和深底偏粉的 red 区分
        expect(p.red).toBe('#c8373a');
    });

    it('auto + 深色背景：自动切深色系 ANSI', () => {
        const p = buildPalette(input(DARK_BG, '#e9e2d8'), 'auto');
        expect(p.background).toBe(DARK_BG);
        expect(p.red).toBe('#f0716b');
        // 两套 ANSI 不能撞色
        const light = buildPalette(input(LIGHT_BG, '#2c1f18'), 'auto');
        expect(p.red).not.toBe(light.red);
        expect(p.black).not.toBe(light.black);
    });

    it('dark：无视输入底色，固定深底，光标仍跟主题强调色', () => {
        const p = buildPalette(input(LIGHT_BG, '#2c1f18'), 'dark');
        expect(p.background).toBe(FIXED_DARK.background);
        expect(p.foreground).toBe(FIXED_DARK.foreground);
        expect(p.cursor).toBe(ACCENT);
        expect(p.cursorAccent).toBe(FIXED_DARK.background);
        expect(p.red).toBe('#f0716b');
    });

    it('选区 / 滚动条 alpha 随明暗与状态分级', () => {
        const dark = buildPalette(input(DARK_BG, '#e9e2d8'), 'auto');
        const light = buildPalette(input(LIGHT_BG, '#2c1f18'), 'auto');
        expect(dark.selectionBackground).toBe(withAlpha(ACCENT, 0.35));
        expect(light.selectionBackground).toBe(withAlpha(ACCENT, 0.3));
        expect(dark.selectionInactiveBackground).toBe(withAlpha(ACCENT, 0.18));
        expect(light.selectionInactiveBackground).toBe(withAlpha(ACCENT, 0.15));
        // 滚动条：静置 < 悬停 < 按下
        expect(dark.scrollbarSliderHoverBackground).toBe(withAlpha(dark.foreground, 0.26));
        for (const p of [dark, light]) {
            const alphaOf = (hex: string) => parseInt(hex.slice(7, 9), 16);
            expect(alphaOf(p.scrollbarSliderBackground)).toBeLessThan(
                alphaOf(p.scrollbarSliderHoverBackground),
            );
            expect(alphaOf(p.scrollbarSliderHoverBackground)).toBeLessThan(
                alphaOf(p.scrollbarSliderActiveBackground),
            );
        }
    });

    it('输出全部是 xterm 可解析的颜色字面量', () => {
        const p = buildPalette(input(LIGHT_BG, '#2c1f18'), 'auto');
        for (const value of Object.values(p)) {
            expect(value).toMatch(/^#[0-9a-f]{6}([0-9a-f]{2})?$/);
        }
    });
});
