// 截图偏好只保存开关和组合键，旧值或损坏值回到默认设置。
export interface ChatScreenshotPreferences {
    addToChat: boolean;
    hideWindow: boolean;
    globalShortcut: boolean;
    shortcut: string;
}

export const SCREENSHOT_PREFERENCES_KEY = 'ncd.chat.screenshot.v1';
export const DEFAULT_SCREENSHOT_PREFERENCES: ChatScreenshotPreferences = {
    addToChat: true,
    hideWindow: true,
    globalShortcut: true,
    shortcut: 'Ctrl+Alt+S',
};

export function normalizeScreenshotShortcut(value: string): string | null {
    const parts = value.split('+');
    const key = parts.pop()?.toUpperCase();
    if (!key || !/^(?:[A-Z0-9]|F(?:[1-9]|1[0-2]))$/.test(key)) return null;
    const modifiers = new Set(parts.map((part) => part.toLowerCase()));
    if (
        !parts.length ||
        modifiers.size !== parts.length ||
        [...modifiers].some((part) => !['ctrl', 'alt', 'shift'].includes(part)) ||
        !['ctrl', 'alt'].some((part) => modifiers.has(part))
    )
        return null;
    return [
        ...['Ctrl', 'Alt', 'Shift'].filter((part) => modifiers.has(part.toLowerCase())),
        key,
    ].join('+');
}

export function parseScreenshotPreferences(raw: string | null): ChatScreenshotPreferences {
    try {
        const value: unknown = JSON.parse(raw ?? 'null');
        if (!value || typeof value !== 'object') return DEFAULT_SCREENSHOT_PREFERENCES;
        const saved = value as Record<string, unknown>;
        return {
            addToChat:
                typeof saved.addToChat === 'boolean'
                    ? saved.addToChat
                    : DEFAULT_SCREENSHOT_PREFERENCES.addToChat,
            globalShortcut:
                typeof saved.globalShortcut === 'boolean'
                    ? saved.globalShortcut
                    : DEFAULT_SCREENSHOT_PREFERENCES.globalShortcut,
            hideWindow:
                typeof saved.hideWindow === 'boolean'
                    ? saved.hideWindow
                    : DEFAULT_SCREENSHOT_PREFERENCES.hideWindow,
            shortcut:
                typeof saved.shortcut === 'string'
                    ? (normalizeScreenshotShortcut(saved.shortcut) ??
                      DEFAULT_SCREENSHOT_PREFERENCES.shortcut)
                    : DEFAULT_SCREENSHOT_PREFERENCES.shortcut,
        };
    } catch {
        return DEFAULT_SCREENSHOT_PREFERENCES;
    }
}

export function screenshotShortcutFromEvent(event: {
    key: string;
    code: string;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    metaKey: boolean;
}): string | null {
    if (event.metaKey) return null;
    const key = /^Key[A-Z]$/.test(event.code)
        ? event.code.slice(3)
        : /^Digit[0-9]$/.test(event.code)
          ? event.code.slice(5)
          : event.key;
    return normalizeScreenshotShortcut(
        [
            ...(event.ctrlKey ? ['Ctrl'] : []),
            ...(event.altKey ? ['Alt'] : []),
            ...(event.shiftKey ? ['Shift'] : []),
            key,
        ].join('+'),
    );
}
