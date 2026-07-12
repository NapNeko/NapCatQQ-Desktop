// NapCat 登录态聚合 store（模块级单例）。
//
// state 持久不丢、跨路由保留；事件订阅在首个 React 订阅者来时挂一次，正常运行
// 不卸载。invalidationEpoch 自动消失定时器（被踢 toast 3s 后淡出）也搬进 store，
// 跟组件树解耦。
//
// 冷启动 / 页面晚于 reconcile 时：拉 list_napcat_webui_bindings 补齐 binding，
// 避免只靠 broadcast 事件导致 WebUI 按钮永不亮（多实例 port+1 场景尤甚）。
// Running 状态变化时再 hydrate 一次，不依赖固定 setTimeout 魔法时间。
//
// 事件走 domain-event-hub，不直接 eventStreamService.subscribe，避免与其它
// 消费者各订一套底层 listen。

import { createStore } from '../utils/createStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { botService } from '../../core/services/bot.service';
import {
    clearInvalidation,
    initialNapcatLoginState,
    reduceNapcatLogin,
    type NapcatLoginState,
} from '../../core/domain/events/login-aggregator';
import type { DomainEvent } from '../../core/ipc/types';

const store = createStore<NapcatLoginState>(initialNapcatLoginState);

const timers: Record<string, ReturnType<typeof setTimeout>> = {};
const lastEpoch: Record<string, number> = {};

let unsubDomain: (() => void) | null = null;
let hydrateInFlight: Promise<void> | null = null;
let hydrateAttempts = 0;
const MAX_EMPTY_HYDRATE_RETRIES = 10;

// 当某个 bot 的 invalidationEpoch 变了，重置 3s 自动消失定时器。
function syncInvalidationTimers(state: NapcatLoginState): void {
    for (const [botId, epoch] of Object.entries(state.invalidationEpoch)) {
        if (lastEpoch[botId] === epoch) continue;
        lastEpoch[botId] = epoch;

        const prev = timers[botId];
        if (prev) clearTimeout(prev);

        timers[botId] = setTimeout(() => {
            store.setState(clearInvalidation(store.getSnapshot(), botId));
            delete timers[botId];
        }, 3000);
    }
}

function applyWebuiBinding(botId: string, port: number, token: string): void {
    const next = reduceNapcatLogin(store.getSnapshot(), {
        kind: 'napcat_webui_available',
        bot_id: botId,
        port,
        token,
    });
    store.setState(next);
}

async function hydrateFromBackend(): Promise<void> {
    try {
        const rows = await botService.listNapcatWebuiBindings();
        for (const row of rows) {
            if (!row.bot_id || !row.port || !row.token) continue;
            const cur = store.getSnapshot().byBot[row.bot_id]?.webui;
            if (cur && cur.port === row.port && cur.token === row.token) continue;
            // 后端表是权威源(stdout 解析的真实口)
            applyWebuiBinding(row.bot_id, row.port, row.token);
        }
        // bootstrap/reconcile 可能仍在写 endpoint:空表时退避再试,有数据后停
        if (rows.length === 0 && hydrateAttempts < MAX_EMPTY_HYDRATE_RETRIES) {
            hydrateAttempts += 1;
            const delayMs = Math.min(250 * hydrateAttempts, 2000);
            window.setTimeout(() => {
                void scheduleHydrate();
            }, delayMs);
        } else if (rows.length > 0) {
            hydrateAttempts = MAX_EMPTY_HYDRATE_RETRIES;
        }
    } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('[napcatLoginStore] hydrate webui bindings failed:', err);
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
    const next = reduceNapcatLogin(store.getSnapshot(), event);
    store.setState(next);
    syncInvalidationTimers(next);

    // reconcile/start 完成后后端 endpoint 表才有值;再拉一次补 UI
    if (
        event.kind === 'bot_state_changed' &&
        (event.snapshot.state === 'running' || event.snapshot.state === 'starting')
    ) {
        void scheduleHydrate();
    }
}

function ensureSubscribed(): void {
    if (unsubDomain) return;
    unsubDomain = subscribeDomainEvents(onDomainEvent);
    void scheduleHydrate();
}

export const napcatLoginStore = {
    getSnapshot: store.getSnapshot,

    subscribe(listener: () => void): () => void {
        ensureSubscribed();
        return store.subscribe(listener);
    },

    // 测试 / dev 重置用。
    _reset(): void {
        for (const t of Object.values(timers)) clearTimeout(t);
        for (const k of Object.keys(timers)) delete timers[k];
        for (const k of Object.keys(lastEpoch)) delete lastEpoch[k];
        if (unsubDomain) {
            try {
                unsubDomain();
            } catch {
                /* noop */
            }
            unsubDomain = null;
        }
        hydrateInFlight = null;
        hydrateAttempts = 0;
        store._reset();
    },
};
