// NapCat 登录态聚合 store（模块级单例）。
//
// state 持久不丢、跨路由保留；事件订阅在首个 React 订阅者来时挂一次，永远不
// 卸载。invalidationEpoch 自动消失定时器（被踢 toast 3s 后淡出）也搬进 store，
// 跟组件树解耦。

import { createStore } from '../utils/createStore';
import { eventStreamService } from '../../core/services/event-stream.service';
import {
    clearInvalidation,
    initialNapcatLoginState,
    reduceNapcatLogin,
    type NapcatLoginState,
} from '../../core/domain/events/login-aggregator';

const store = createStore<NapcatLoginState>(initialNapcatLoginState);

const timers: Record<string, ReturnType<typeof setTimeout>> = {};
const lastEpoch: Record<string, number> = {};

let subscribePromise: Promise<() => void> | null = null;

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

function ensureSubscribed(): void {
    if (subscribePromise) return;
    subscribePromise = eventStreamService.subscribe((event) => {
        const next = reduceNapcatLogin(store.getSnapshot(), event);
        store.setState(next);
        syncInvalidationTimers(next);
    });
}

export const napcatLoginStore = {
    getSnapshot: store.getSnapshot,

    subscribe(listener: () => void): () => void {
        ensureSubscribed();
        return store.subscribe(listener);
    },

    /** 测试 / dev 重置用。 */
    _reset(): void {
        for (const t of Object.values(timers)) clearTimeout(t);
        for (const k of Object.keys(timers)) delete timers[k];
        for (const k of Object.keys(lastEpoch)) delete lastEpoch[k];
        store._reset();
    },
};
