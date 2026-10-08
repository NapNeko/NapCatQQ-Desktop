import { describe, expect, it } from 'vitest';
import {
    DEFAULT_SCREENSHOT_PREFERENCES,
    normalizeScreenshotShortcut,
    parseScreenshotPreferences,
    screenshotShortcutFromEvent,
} from './chatScreenshotPreferences';

describe('screenshot preferences', () => {
    it('restores safe defaults for corrupt or older stored values', () => {
        expect(parseScreenshotPreferences('{')).toEqual(DEFAULT_SCREENSHOT_PREFERENCES);
        expect(parseScreenshotPreferences(null)).toEqual(DEFAULT_SCREENSHOT_PREFERENCES);
        expect(parseScreenshotPreferences('{"hideWindow":false}')).toEqual({
            hideWindow: false,
            globalShortcut: true,
            shortcut: 'Ctrl+Alt+S',
        });
        expect(parseScreenshotPreferences('{"shortcut":"Meta+S"}').shortcut).toBe('Ctrl+Alt+S');
    });

    it('defaults older settings to global and preserves explicit focus-only scope', () => {
        expect(parseScreenshotPreferences('{"shortcut":"Ctrl+Alt+S"}').globalShortcut).toBe(true);
        expect(parseScreenshotPreferences('{"globalShortcut":false}').globalShortcut).toBe(false);
        expect(DEFAULT_SCREENSHOT_PREFERENCES.globalShortcut).toBe(true);
    });

    it('normalizes valid native combinations in a stable modifier order', () => {
        expect(normalizeScreenshotShortcut('shift+alt+ctrl+s')).toBe('Ctrl+Alt+Shift+S');
        expect(normalizeScreenshotShortcut('Alt+F12')).toBe('Alt+F12');
        expect(normalizeScreenshotShortcut('Ctrl+9')).toBe('Ctrl+9');
    });

    it.each([
        'S',
        'Shift+S',
        'Meta+S',
        'Ctrl+Meta+S',
        'Ctrl+Ctrl+S',
        'Alt+PrintScreen',
        'Ctrl+F13',
        'Ctrl+ArrowLeft',
        'Ctrl++',
        'Ctrl+Space',
    ])('rejects unsupported or incomplete shortcut %s', (shortcut) => {
        expect(normalizeScreenshotShortcut(shortcut)).toBeNull();
    });

    it('uses physical letter and number keys with a non-Latin keyboard layout', () => {
        const modifiers = { ctrlKey: true, altKey: true, shiftKey: false, metaKey: false };
        expect(screenshotShortcutFromEvent({ ...modifiers, key: 'ы', code: 'KeyS' })).toBe(
            'Ctrl+Alt+S',
        );
        expect(screenshotShortcutFromEvent({ ...modifiers, key: '&', code: 'Digit1' })).toBe(
            'Ctrl+Alt+1',
        );
        expect(
            screenshotShortcutFromEvent({ ...modifiers, metaKey: true, key: 's', code: 'KeyS' }),
        ).toBeNull();
    });
});
