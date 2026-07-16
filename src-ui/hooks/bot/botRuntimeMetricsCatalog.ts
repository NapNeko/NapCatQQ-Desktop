// 全列表共享的实例指标目录：一次 list_bot_runtime_metrics，多卡订阅。
// 页面不可见时降频轮询，避免后台狂刷 IPC。

import { botService } from '../../core/services/bot.service';
import { settingsService } from '../../core/services/settings.service';
import type { BotRuntimeMetrics } from '../../core/ipc/generated/domain/BotRuntimeMetrics';
import {
    BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
    BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
    clampBotRuntimeMetricsIntervalMs,
    clampBotRuntimeMetricsRetentionDays,
} from '../../core/domain/bot/runtime-metrics-settings';

export interface BotRuntimeMetricsCatalogState {
    enabled: boolean;
    intervalMs: number;
    retentionDays: number;
    byId: Record<string, BotRuntimeMetrics>;
    lastFetchAt: number;
    fetching: boolean;
}

type Listener = () => void;

const HIDDEN_POLL_FACTOR = 4;

let state: BotRuntimeMetricsCatalogState = {
    enabled: false,
    intervalMs: BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
    retentionDays: BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
    byId: {},
    lastFetchAt: 0,
    fetching: false,
};

const listeners = new Set<Listener>();
let pollTimer: number | null = null;
let settingsLoaded = false;
let settingsInflight: Promise<void> | null = null;

function emit() {
    for (const l of listeners) l();
}

function setState(patch: Partial<BotRuntimeMetricsCatalogState>) {
    state = { ...state, ...patch };
    emit();
}

async function ensureSettings(): Promise<void> {
    if (settingsLoaded) return;
    if (settingsInflight) return settingsInflight;
    settingsInflight = settingsService
        .get()
        .then((s) => {
            setState({
                enabled: !!s.botRuntimeMetricsEnabled,
                intervalMs: clampBotRuntimeMetricsIntervalMs(
                    Number(s.botRuntimeMetricsIntervalMs) ||
                    BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
                ),
                retentionDays: clampBotRuntimeMetricsRetentionDays(
                    Number(s.botRuntimeMetricsRetentionDays) ||
                    BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
                ),
            });
            settingsLoaded = true;
        })
        .catch(() => {
            settingsLoaded = true;
        })
        .finally(() => {
            settingsInflight = null;
        });
    return settingsInflight;
}

async function fetchList(): Promise<void> {
    await ensureSettings();
    if (!state.enabled) {
        if (Object.keys(state.byId).length > 0) {
            setState({ byId: {}, fetching: false });
        }
        return;
    }
    setState({ fetching: true });
    try {
        const list = await botService.listRuntimeMetrics();
        const byId: Record<string, BotRuntimeMetrics> = {};
        for (const m of list) {
            const id = String(m.bot_id ?? '');
            if (id) byId[id] = m;
        }
        setState({ byId, lastFetchAt: Date.now(), fetching: false });
    } catch {
        setState({ fetching: false });
    }
}

function pollIntervalMs(): number {
    const base = Math.max(1000, state.intervalMs || BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT);
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
        return base * HIDDEN_POLL_FACTOR;
    }
    return base;
}

function clearPoll() {
    if (pollTimer != null) {
        window.clearTimeout(pollTimer);
        pollTimer = null;
    }
}

function scheduleNext() {
    clearPoll();
    if (listeners.size === 0 || !state.enabled) return;
    pollTimer = window.setTimeout(() => {
        void fetchList().finally(() => scheduleNext());
    }, pollIntervalMs());
}

function onVisibility() {
    if (listeners.size === 0) return;
    // 回到前台立刻拉一次，并按可见间隔重排
    void fetchList().finally(() => scheduleNext());
}

function startIfNeeded() {
    if (listeners.size === 0) return;
    void ensureSettings().then(() => {
        if (!state.enabled) {
            clearPoll();
            return;
        }
        void fetchList().finally(() => scheduleNext());
    });
    if (typeof document !== 'undefined') {
        document.addEventListener('visibilitychange', onVisibility);
    }
}

function stopIfIdle() {
    if (listeners.size > 0) return;
    clearPoll();
    if (typeof document !== 'undefined') {
        document.removeEventListener('visibilitychange', onVisibility);
    }
}

export function getBotRuntimeMetricsCatalogSnapshot(): BotRuntimeMetricsCatalogState {
    return state;
}

export function subscribeBotRuntimeMetricsCatalog(listener: Listener): () => void {
    listeners.add(listener);
    if (listeners.size === 1) startIfNeeded();
    return () => {
        listeners.delete(listener);
        stopIfIdle();
    };
}

/** 设置页保存后可调用，强制重读开关并刷新 */
export async function reloadBotRuntimeMetricsCatalogSettings(): Promise<void> {
    settingsLoaded = false;
    await ensureSettings();
    if (listeners.size > 0) {
        await fetchList();
        scheduleNext();
    }
}

export async function refreshBotRuntimeMetricsCatalog(): Promise<void> {
    await fetchList();
}
