import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const runtime = vi.hoisted(() => ({
    isTauri: true,
    os: 'linux' as 'linux' | 'windows',
}));
const charge = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('../domain/runtime/env', () => ({
    get isTauri() {
        return runtime.isTauri;
    },
    localOs: () => runtime.os,
}));
vi.mock('./themeCharge', () => ({ runThemeCharge: charge }));

const originalViewTransition = Object.getOwnPropertyDescriptor(document, 'startViewTransition');
const startViewTransition = vi.fn((update: () => void) => {
    update();
    return { ready: Promise.resolve(), finished: Promise.resolve() };
});

beforeEach(() => {
    vi.resetModules();
    runtime.isTauri = true;
    runtime.os = 'linux';
    document.startViewTransition = startViewTransition;
    vi.stubGlobal('CSS', { registerProperty: vi.fn() });
});

afterEach(() => {
    if (originalViewTransition) {
        Object.defineProperty(document, 'startViewTransition', originalViewTransition);
    } else {
        delete document.startViewTransition;
    }
    document.documentElement.removeAttribute('data-theme');
    document.getElementById('ndf-theme-reveal-hole-keyframes')?.remove();
    vi.unstubAllGlobals();
});

describe('playThemeTransition', () => {
    it.each(['standard', 'rich'] as const)('Linux Tauri 的 %s 档直接应用主题', async (level) => {
        const { playThemeTransition } = await import('./themeTransition');
        const changeTheme = vi.fn(() => {
            document.documentElement.dataset.theme = 'dark';
        });

        await playThemeTransition(changeTheme, {
            enabled: true,
            level,
            duration: 1,
            easing: 'ease-in-out',
        });

        expect(changeTheme).toHaveBeenCalledOnce();
        expect(document.documentElement.dataset.theme).toBe('dark');
        expect(startViewTransition).not.toHaveBeenCalled();
        expect(charge).not.toHaveBeenCalled();
        expect(document.documentElement.hasAttribute('data-theme-reveal')).toBe(false);
        expect(document.querySelector('.ndf-theme-origin-ring')).toBeNull();
        expect(document.getElementById('ndf-theme-reveal-hole-keyframes')).toBeNull();
    });

    it.each([
        { name: 'Windows Tauri', isTauri: true, os: 'windows' as const },
        { name: 'Linux 浏览器', isTauri: false, os: 'linux' as const },
    ])('$name 保留主题动画并清理临时元素', async ({ isTauri, os }) => {
        runtime.isTauri = isTauri;
        runtime.os = os;
        const { playThemeTransition } = await import('./themeTransition');
        const changeTheme = vi.fn();

        await playThemeTransition(changeTheme, {
            enabled: true,
            level: 'standard',
            duration: 1,
            easing: 'ease-in-out',
        });

        expect(changeTheme).toHaveBeenCalledOnce();
        expect(startViewTransition).toHaveBeenCalledOnce();
        expect(charge).toHaveBeenCalledOnce();
        expect(document.documentElement.hasAttribute('data-theme-reveal')).toBe(false);
        expect(document.documentElement.style.getPropertyValue('--theme-reveal-dur')).toBe('');
        expect(document.querySelector('.ndf-theme-origin-ring')).toBeNull();
    });
});
