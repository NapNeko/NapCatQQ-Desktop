// AppSettings.uiPreferences ↔ preferencesStore 形状互转（单一规范化入口）。

import type { AppUiPreferences } from '../../ipc/generated/domain/AppUiPreferences';
import {
    MOTION_SPEED_DEFAULT,
    MOTION_SPEED_MAX,
    MOTION_SPEED_MIN,
} from '../../design/motion';
import {
    RADIUS_STYLE_DEFAULT,
    normalizeRadiusStyle,
} from '../../design/radius';
import type { AppPreferences, CloseAction, ThemeMode } from '../../../hooks/preferences/preferencesStore';
import {
    normalizeCloseAction,
} from '../../../hooks/preferences/preferencesStore';

const VALID_THEMES: ReadonlySet<ThemeMode> = new Set<ThemeMode>([
    'auto', 'light', 'dark', 'latte', 'frappe', 'macchiato', 'mocha',
]);

function normalizeTheme(raw: unknown): ThemeMode {
    return typeof raw === 'string' && VALID_THEMES.has(raw as ThemeMode)
        ? (raw as ThemeMode)
        : 'auto';
}

function normalizeMotionLevel(raw: unknown): AppPreferences['motionLevel'] {
    return raw === 'elegant' || raw === 'rich' ? raw : 'standard';
}

function normalizeMotionSpeed(raw: unknown): number {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return MOTION_SPEED_DEFAULT;
    return Math.max(MOTION_SPEED_MIN, Math.min(MOTION_SPEED_MAX, raw));
}

export function appUiPreferencesToAppPreferences(
    ui: AppUiPreferences,
    closeAction: CloseAction,
): AppPreferences {
    return {
        theme: normalizeTheme(ui.theme),
        showMascot: ui.showMascot !== false,
        closeAction,
        motionEnabled: ui.motionEnabled !== false,
        motionLevel: normalizeMotionLevel(ui.motionLevel),
        motionSpeed: normalizeMotionSpeed(ui.motionSpeed),
        radiusStyle: normalizeRadiusStyle(ui.radiusStyle),
    };
}

export function appPreferencesToAppUiPreferences(prefs: AppPreferences): AppUiPreferences {
    return {
        theme: prefs.theme,
        showMascot: prefs.showMascot,
        motionEnabled: prefs.motionEnabled,
        motionLevel: prefs.motionLevel,
        motionSpeed: prefs.motionSpeed,
        radiusStyle: prefs.radiusStyle,
    };
}

/** 磁盘缺 uiPreferences（旧版 app-settings.json）时用当前 localStorage 偏好回填。 */
export function defaultAppUiPreferencesFromPrefs(prefs: AppPreferences): AppUiPreferences {
    return appPreferencesToAppUiPreferences(prefs);
}

export function closeActionFromDto(raw: unknown): CloseAction {
    return normalizeCloseAction(raw);
}

/** 判断磁盘上的 ui 是否与默认结构等价（用于决定是否用 localStorage 迁移）。 */
export function isDefaultUiPreferencesOnDisk(ui: AppUiPreferences): boolean {
    return (
        normalizeTheme(ui.theme) === 'auto' &&
        ui.showMascot !== false &&
        ui.motionEnabled !== false &&
        normalizeMotionLevel(ui.motionLevel) === 'standard' &&
        normalizeMotionSpeed(ui.motionSpeed) === MOTION_SPEED_DEFAULT &&
        normalizeRadiusStyle(ui.radiusStyle) === RADIUS_STYLE_DEFAULT
    );
}