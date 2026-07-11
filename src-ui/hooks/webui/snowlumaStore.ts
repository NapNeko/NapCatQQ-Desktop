// SnowLuma daemon + per-bot 聚合 store（模块级单例）。
// state 持久不丢，事件订阅一次（首个 React 订阅者来时挂上），路由切回不重置。
//
// 冷启动 / 页面晚于 reconcile 时：拉 list_snowluma_ui_snapshot 补齐
// daemonState / loginState / endpointsReady，避免只靠 broadcast 丢事件
// 导致 WebUI 按钮与登录徽章永不恢复（对齐 napcatLoginStore hydrate）。

import { createStore } from '../utils/createStore';
import { eventStreamService } from '../../core/services/event-stream.service';
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

let subscribePromise: Promise<() => void> | null = null;
let hydrateInFlight: Promise<void> | null = null;
let hydrateAttempts = 0;
const MAX_EMPTY_HYDRATE_RETRIES = 10;

const emptyBot = (): SnowlumaBotState => ({
    injected: false,
    uin: null,
    loginState: null,
    dockerEndpointsReady: false,
});

function ensureSubscribed(): void {
    if (subscribePromise) return;
    subscribePromise = eventStreamService.subscribe((event) => {
        onDomainEvent(event);
    });
    void scheduleHydrate();
}

function isDaemonState(v: unknown): v is DaemonState {
    return (
        v === 'stopped' ||
        v === 'starting' ||
        v === 'ready' ||
        v === 'stopping' ||
        v === 'crashed'
    );
}

function isLoginState(v: unknown): v is SnowLumaLoginState {
    return (
        v === 'starting' ||
        v === 'waiting_for_qr_scan' ||
        v === 'logged_in' ||
        v === 'disconnected'
    );
}

async function hydrateFromBackend(): Promise<void> {
    try {
        const snap = await botService.listSnowlumaUiSnapshot();
        const prev = store.getSnapshot();
        const nextByBot: Record<string, SnowlumaBotState> = { ...prev.byBot };

        for (const row of snap.bots) {
            if (!row.bot_id) continue;
            const cur = nextByBot[row.bot_id] ?? emptyBot();
            nextByBot[row.bot_id] = {
                ...cur,
                injected: row.injected,
                uin: row.uin,
                loginState: isLoginState(row.login_state) ? row.login_state : cur.loginState,
                dockerEndpointsReady: row.endpoints_ready,
            };
        }

        store.setState({
            daemonState: isDaemonState(snap.daemon_state)
                ? snap.daemon_state
                : prev.daemonState,
            byBot: nextByBot,
        });

        const hasAny =
            isDaemonState(snap.daemon_state) ||
            snap.bots.some(
                (b) => b.injected || b.endpoints_ready || b.login_state || b.uin,
            );
        // reconcile 可能仍在写表:空时退避再试,有数据后停
        if (!hasAny && hydrateAttempts < MAX_EMPTY_HYDRATE_RETRIES) {
            hydrateAttempts += 1;
            const delayMs = Math.min(250 * hydrateAttempts, 2000);
            window.setTimeout(() => {
                void scheduleHydrate();
            }, delayMs);
        } else if (hasAny) {
            hydrateAttempts = MAX_EMPTY_HYDRATE_RETRIES;
        }
    } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('[snowlumaStore] hydrate ui snapshot failed:', err);
    }
}

function scheduleHydrate(): Promise<void> {
    if (hydrateInFlight) return hydrateInFlight;
    hydrateInFlight = hydrateFromBackend().finally(() => {
        hydrateInFlight = null;
    });
    return hydrateInFlight;
}

function onDomainEvent(event: DomainEvent): void {
    const next = reduceSnowluma(store.getSnapshot(), event);
    store.setState(next);

    // reconcile/start 完成后后端表才有值;再拉一次补 UI
    if (
        event.kind === 'bot_state_changed' &&
        (event.snapshot.state === 'running' || event.snapshot.state === 'starting')
    ) {
        void scheduleHydrate();
    }
}

export const snowlumaStore = {
    getSnapshot: store.getSnapshot,

    subscribe(listener: () => void): () => void {
        ensureSubscribed();
        return store.subscribe(listener);
    },

    /** 测试 / dev 重置用。 */
    _reset(): void {
        store._reset();
        hydrateAttempts = 0;
        hydrateInFlight = null;
    },
};
