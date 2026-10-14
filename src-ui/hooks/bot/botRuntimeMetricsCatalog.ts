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
let settingsGeneration = 0;
let listInflight: Promise<void> | null = null;

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
    const generation = settingsGeneration;
    const request = Promise.resolve()
        .then(() => settingsService.get())
        .then((s) => {
            if (generation !== settingsGeneration) return;
            setState({
                enabled: !!s.botRuntimeMetricsEnabled,
                ...(!s.botRuntimeMetricsEnabled ? { byId: {}, fetching: false } : {}),
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
            // 暂时读不到设置时下一轮再试，不能把初始的 false 永久当成用户关闭了监控。
        })
        .finally(() => {
            if (settingsInflight === request) settingsInflight = null;
        });
    settingsInflight = request;
    return request;
}

async function fetchList(): Promise<void> {
    const generation = settingsGeneration;
    await ensureSettings();
    if (generation !== settingsGeneration) return;
    if (!state.enabled) {
        if (Object.keys(state.byId).length > 0) {
            setState({ byId: {}, fetching: false });
        }
        return;
    }
    if (listInflight) return listInflight;
    const request = Promise.resolve()
        .then(() => botService.listRuntimeMetrics())
        .then((list) => {
            if (generation !== settingsGeneration) return;
            const byId: Record<string, BotRuntimeMetrics> = {};
            for (const m of list) {
                const id = String(m.bot_id ?? '');
                if (id) byId[id] = m;
            }
            setState({ byId, lastFetchAt: Date.now() });
        })
        .catch(() => {
            // 保留最近一次成功采样，采集时间仍可说明它已经多久没有更新。
        })
        .finally(() => {
            if (listInflight === request) {
                listInflight = null;
                setState({ fetching: false });
            }
        });
    listInflight = request;
    setState({ fetching: true });
    return request;
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
    if (listeners.size === 0 || (!state.enabled && settingsLoaded)) return;
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
    void fetchList().finally(() => scheduleNext());
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
    // 已发出的 IPC 继续跑，但只能由新设置这一代写回，旧请求也不能清掉新请求的 fetching。
    const generation = ++settingsGeneration;
    settingsLoaded = false;
    settingsInflight = null;
    listInflight = null;
    clearPoll();
    setState({ fetching: false });
    await ensureSettings();
    if (generation !== settingsGeneration) return;
    if (listeners.size > 0) {
        await fetchList();
        scheduleNext();
    }
}

export async function refreshBotRuntimeMetricsCatalog(): Promise<void> {
    await fetchList();
}
