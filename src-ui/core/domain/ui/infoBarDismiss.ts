// InfoBar 自动关闭：danger 永不自动关；其余 tone 由 app-settings.uiPreferences 配置。
// 磁盘 0 = 关闭自动关闭；开启时时长 1000–60000 ms，步进 100。

import type { InfoBarTone } from '../../../shared/ui/InfoBar';
import type { AppUiPreferences } from '../../ipc/generated/domain/AppUiPreferences';

export const INFOBAR_DISMISS_MS_OFF = 0;
export const INFOBAR_DISMISS_SLIDER_MIN = 1000;
export const INFOBAR_DISMISS_SLIDER_MAX = 60_000;
export const INFOBAR_DISMISS_SLIDER_STEP = 100;

/** @deprecated 使用 INFOBAR_DISMISS_SLIDER_MAX */
export const INFOBAR_DISMISS_MS_MAX = INFOBAR_DISMISS_SLIDER_MAX;

export type InfoBarDismissPrefs = {
    infoBarDismissInfoMs: number;
    infoBarDismissSuccessMs: number;
    infoBarDismissWarningMs: number;
};

export const DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED = {
    infoBarDismissInfoMs: 5000,
    infoBarDismissSuccessMs: 4000,
    infoBarDismissWarningMs: 6000,
} as const;

export const DEFAULT_INFOBAR_DISMISS: InfoBarDismissPrefs = {
    infoBarDismissInfoMs: DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs,
    infoBarDismissSuccessMs: DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs,
    infoBarDismissWarningMs: DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs,
};

export type InfoBarDismissDraftSlice = {
    infoBarDismissInfoEnabled: boolean;
    infoBarDismissInfoMs: number;
    infoBarDismissSuccessEnabled: boolean;
    infoBarDismissSuccessMs: number;
    infoBarDismissWarningEnabled: boolean;
    infoBarDismissWarningMs: number;
};

export function clampInfoBarDismissSliderMs(raw: unknown): number {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) {
        return DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs;
    }
    const stepped =
        Math.round(raw / INFOBAR_DISMISS_SLIDER_STEP) * INFOBAR_DISMISS_SLIDER_STEP;
    return Math.max(
        INFOBAR_DISMISS_SLIDER_MIN,
        Math.min(INFOBAR_DISMISS_SLIDER_MAX, stepped),
    );
}

/** 落盘 / push 解析：0 合法表示不自动关。 */
export function clampInfoBarDismissStoredMs(raw: unknown): number {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return INFOBAR_DISMISS_MS_OFF;
    const n = Math.round(raw);
    if (n <= 0) return INFOBAR_DISMISS_MS_OFF;
    return clampInfoBarDismissSliderMs(n);
}

export function infoBarDismissFromUiPreferences(
    ui: AppUiPreferences,
): InfoBarDismissPrefs {
    const toMs = (v: bigint | number | undefined): number => {
        if (typeof v === 'bigint') return clampInfoBarDismissStoredMs(Number(v));
        return clampInfoBarDismissStoredMs(v);
    };
    return {
        infoBarDismissInfoMs: toMs(ui.infoBarDismissInfoMs),
        infoBarDismissSuccessMs: toMs(ui.infoBarDismissSuccessMs),
        infoBarDismissWarningMs: toMs(ui.infoBarDismissWarningMs),
    };
}

export function infoBarDismissDraftFromStored(
    prefs: InfoBarDismissPrefs,
): InfoBarDismissDraftSlice {
    const msOrDefault = (
        stored: number,
        fallback: number,
    ): number =>
        stored > 0 ? clampInfoBarDismissSliderMs(stored) : fallback;

    return {
        infoBarDismissInfoEnabled: prefs.infoBarDismissInfoMs > 0,
        infoBarDismissInfoMs: msOrDefault(
            prefs.infoBarDismissInfoMs,
            DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs,
        ),
        infoBarDismissSuccessEnabled: prefs.infoBarDismissSuccessMs > 0,
        infoBarDismissSuccessMs: msOrDefault(
            prefs.infoBarDismissSuccessMs,
            DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs,
        ),
        infoBarDismissWarningEnabled: prefs.infoBarDismissWarningMs > 0,
        infoBarDismissWarningMs: msOrDefault(
            prefs.infoBarDismissWarningMs,
            DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs,
        ),
    };
}

export function infoBarDismissPrefsFromDraft(
    draft: InfoBarDismissDraftSlice,
): InfoBarDismissPrefs {
    return {
        infoBarDismissInfoMs: draft.infoBarDismissInfoEnabled
            ? clampInfoBarDismissSliderMs(draft.infoBarDismissInfoMs)
            : INFOBAR_DISMISS_MS_OFF,
        infoBarDismissSuccessMs: draft.infoBarDismissSuccessEnabled
            ? clampInfoBarDismissSliderMs(draft.infoBarDismissSuccessMs)
            : INFOBAR_DISMISS_MS_OFF,
        infoBarDismissWarningMs: draft.infoBarDismissWarningEnabled
            ? clampInfoBarDismissSliderMs(draft.infoBarDismissWarningMs)
            : INFOBAR_DISMISS_MS_OFF,
    };
}

export function defaultAutoDismissMsForTone(
    tone: InfoBarTone | null | undefined,
    prefs: InfoBarDismissPrefs,
): number {
    const t = tone ?? 'info';
    if (t === 'danger') return 0;
    switch (t) {
        case 'success':
            return prefs.infoBarDismissSuccessMs;
        case 'warning':
            return prefs.infoBarDismissWarningMs;
        case 'info':
        default:
            return prefs.infoBarDismissInfoMs;
    }
}

/** push 时：显式 autoDismissMs 优先；danger 强制 0；未传则用 prefs。 */
export function resolveInfoBarAutoDismissMs(
    tone: InfoBarTone | null | undefined,
    explicit: number | undefined,
    prefs: InfoBarDismissPrefs,
): number {
    if ((tone ?? 'info') === 'danger') return 0;
    if (explicit !== undefined) return clampInfoBarDismissStoredMs(explicit);
    return defaultAutoDismissMsForTone(tone, prefs);
}