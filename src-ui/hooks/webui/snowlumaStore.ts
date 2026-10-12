// SnowLuma daemon + per-bot 聚合 store（模块级单例）。
// state 持久不丢，事件订阅一次（首个 React 订阅者来时挂上），路由切回不重置。
//
// 冷启动 / 页面晚于 reconcile 时：拉 list_snowluma_ui_snapshot 补齐
// daemonStates / loginState / endpointsReady，避免只靠 broadcast 丢事件
// 导致 WebUI 按钮与登录徽章永不恢复（对齐 napcatLoginStore hydrate）。
//
// 事件走 domain-event-hub，不直接 eventStreamService.subscribe。

import { createStore } from '../utils/createStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { botService } from '../../core/services/bot.service';
import {
    initialSnowlumaState,
    reduceSnowluma,
    type SnowlumaBotState,
    type SnowlumaState,
} from '../../core/domain/events/snowluma-aggregator';
import type { DaemonState } from '../../core/ipc/generated/DaemonState';
import type { SnowLumaLoginState } from '../../core/ipc/generated/SnowLumaLoginState';
import type { DomainEvent } from '../../core/ipc/types';

const store = createStore<SnowlumaState>(initialSnowlumaState);

let unsubDomain: (() => void) | null = null;
let hydrateInFlight: Promise<void> | null = null;
let hydrateAttempts = 0;
let eventRevision = 0;
let hydrateEpoch = 0;
let fullHydratePending = false;
const hydrateTargets = new Set<string>();
let hydrateRetryTimer: number | null = null;
const MAX_EMPTY_HYDRATE_RETRIES = 10;

function ensureSubscribed(): void {
    if (unsubDomain) return;
    unsubDomain = subscribeDomainEvents(onDomainEvent);
    void scheduleHydrate();
}

function isDaemonState(v: unknown): v is DaemonState {
    return (
        v === 'stopped' || v === 'starting' || v === 'ready' || v === 'stopping' || v === 'crashed'
    );
}

function isLoginState(v: unknown): v is SnowLumaLoginState {
    return (
        v === 'starting' || v === 'waiting_for_qr_scan' || v === 'logged_in' || v === 'disconnected'
    );
}

async function hydrateFromBackend(
    targets: ReadonlySet<string> | null,
    epoch: number,
): Promise<void> {
    const revisionAtStart = eventRevision;
    try {
        const snap = await botService.listSnowlumaUiSnapshot();
        // hydrate 期间若收到更新事件，不能让较旧 command 响应覆盖实时状态。
        if (eventRevision !== revisionAtStart || hydrateEpoch !== epoch) return;

        const current = store.getSnapshot();
        const nextByBot: Record<string, SnowlumaBotState> = { ...current.byBot };

        for (const row of snap.bots) {
            if (!row.bot_id || (targets && !targets.has(row.bot_id))) continue;
            const previous = current.byBot[row.bot_id];
            const next: SnowlumaBotState = {
                injected: row.injected,
                uin: row.uin,
                loginState: isLoginState(row.login_state) ? row.login_state : null,
                dockerEndpointsReady: row.endpoints_ready,
                probeUnavailable: 'probe_unavailable' in row && row.probe_unavailable === true,
            };
            // 失败/缺席快照不是退出事件，不能清掉仍然有效的账号确认。
            nextByBot[row.bot_id] =
                previous?.loginState === 'logged_in' && next.loginState === null
                    ? { ...previous, probeUnavailable: next.probeUnavailable }
                    : next;
        }

        const daemonStates: Record<string, DaemonState> = { ...current.daemonStates };
        for (const [scope, state] of Object.entries(snap.daemon_states ?? {})) {
            if (scope && isDaemonState(state) && (!targets || !daemonStates[scope])) {
                daemonStates[scope] = state;
            }
        }
        // 兼容旧 Desktop command 响应。
        if (!daemonStates.local && isDaemonState(snap.daemon_state)) {
            daemonStates.local = snap.daemon_state;
        }

        store.setState({
            daemonStates,
            byBot: nextByBot,
        });

        const hasAny =
            Object.keys(daemonStates).length > 0 ||
            snap.bots.some((b) => b.injected || b.endpoints_ready || b.login_state || b.uin);
        // reconcile 可能仍在写表:空时退避再试,有数据后停
        if (!targets && !hasAny && hydrateAttempts < MAX_EMPTY_HYDRATE_RETRIES) {
            hydrateAttempts += 1;
            const delayMs = Math.min(250 * hydrateAttempts, 2000);
            hydrateRetryTimer = window.setTimeout(() => {
                hydrateRetryTimer = null;
                void scheduleHydrate();
            }, delayMs);
        } else if (hasAny) {
            hydrateAttempts = MAX_EMPTY_HYDRATE_RETRIES;
            if (hydrateRetryTimer !== null) window.clearTimeout(hydrateRetryTimer);
            hydrateRetryTimer = null;
        }
    } catch (err) {
        console.warn('[snowlumaStore] hydrate ui snapshot failed:', err);
    }
}

function scheduleHydrate(botId?: string): Promise<void> {
    if (botId) hydrateTargets.add(botId);
    else fullHydratePending = true;
    if (hydrateInFlight) return hydrateInFlight;
    const epoch = hydrateEpoch;
    const task = (async () => {
        while (hydrateEpoch === epoch && (fullHydratePending || hydrateTargets.size > 0)) {
            const targets = fullHydratePending ? null : new Set(hydrateTargets);
            fullHydratePending = false;
            hydrateTargets.clear();
            await hydrateFromBackend(targets, epoch);
        }
    })().finally(() => {
        if (hydrateInFlight !== task) return;
        hydrateInFlight = null;
        if (fullHydratePending || hydrateTargets.size > 0) {
            void scheduleHydrate(hydrateTargets.values().next().value);
        }
    });
    hydrateInFlight = task;
    return task;
}

function onDomainEvent(event: DomainEvent): void {
    const current = store.getSnapshot();
    const next = reduceSnowluma(current, event);
    if (
        next !== current ||
        event.kind === 'bot_process_exited' ||
        (event.kind === 'bot_state_changed' &&
            (event.snapshot.state === 'stopped' || event.snapshot.state === 'crashed'))
    ) {
        eventRevision += 1;
    }
    store.setState(next);

    // reconcile/start 完成后后端表才有值;再拉一次补 UI
    if (
        event.kind === 'bot_state_changed' &&
        (event.snapshot.state === 'running' || event.snapshot.state === 'starting')
    ) {
        void scheduleHydrate(event.snapshot.bot_id);
    }
}

export const snowlumaStore = {
    getSnapshot: store.getSnapshot,

    subscribe(listener: () => void): () => void {
        ensureSubscribed();
        return store.subscribe(listener);
    },

    // 测试 / dev 重置用。
    _reset(): void {
        if (unsubDomain) {
            try {
                unsubDomain();
            } catch {
                /* noop */
            }
            unsubDomain = null;
        }
        hydrateAttempts = 0;
        eventRevision = 0;
        hydrateEpoch += 1;
        hydrateInFlight = null;
        fullHydratePending = false;
        hydrateTargets.clear();
        if (hydrateRetryTimer !== null) window.clearTimeout(hydrateRetryTimer);
        hydrateRetryTimer = null;
        store._reset();
    },
};
