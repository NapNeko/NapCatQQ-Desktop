// NapCat 登录态聚合 store（模块级单例）。
//
// 跟旧 useNapcatLogin 的差别：state 不再 useReducer 持有，路由切走再切回组件
// 重新 mount 时 state 还在；事件订阅在 store 第一次被读到时启动一次，永远不
// 卸载，后台累积的事件不丢。invalidationEpoch 自动消失定时器也搬进 store，
// 跟组件树解耦。
//
// 形态参考 globalInfoBarStore：state + listeners + useSyncExternalStore。

import { eventStreamService } from '../../core/services/event-stream.service';
import {
    clearInvalidation,
    initialNapcatLoginState,
    reduceNapcatLogin,
    type NapcatLoginState,
} from '../../core/domain/events/login-aggregator';

let state: NapcatLoginState = initialNapcatLoginState;
const listeners = new Set<() => void>();

const timers: Record<string, ReturnType<typeof setTimeout>> = {};
const lastEpoch: Record<string, number> = {};

let subscribePromise: Promise<() => void> | null = null;

function emit(): void {
    for (const fn of listeners) fn();
}

function setState(next: NapcatLoginState): void {
    if (next === state) return;
    state = next;
    syncInvalidationTimers();
    emit();
}

// 当某个 bot 的 invalidationEpoch 变了，重置 3s 自动消失定时器。
function syncInvalidationTimers(): void {
    for (const [botId, epoch] of Object.entries(state.invalidationEpoch)) {
        if (lastEpoch[botId] === epoch) continue;
        lastEpoch[botId] = epoch;

        const prev = timers[botId];
        if (prev) clearTimeout(prev);

        timers[botId] = setTimeout(() => {
            state = clearInvalidation(state, botId);
            delete timers[botId];
            emit();
        }, 3000);
    }
}

function ensureSubscribed(): void {
    if (subscribePromise) return;
    subscribePromise = eventStreamService.subscribe((event) => {
        setState(reduceNapcatLogin(state, event));
    });
}

export const napcatLoginStore = {
    getSnapshot(): NapcatLoginState {
        return state;
    },

    subscribe(listener: () => void): () => void {
        ensureSubscribed();
        listeners.add(listener);
        return () => {
            listeners.delete(listener);
        };
    },

    /** 测试 / dev 重置用。 */
    _reset(): void {
        state = initialNapcatLoginState;
        for (const t of Object.values(timers)) clearTimeout(t);
        for (const k of Object.keys(timers)) delete timers[k];
        for (const k of Object.keys(lastEpoch)) delete lastEpoch[k];
        emit();
    },
};
