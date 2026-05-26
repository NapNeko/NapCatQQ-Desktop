// SnowLuma daemon + per-bot 聚合 store（模块级单例）。
// 跟 napcatLoginStore 同型：state 持久不丢，事件订阅一次，路由切回不重置。

import { eventStreamService } from '../../core/services/event-stream.service';
import {
    initialSnowlumaState,
    reduceSnowluma,
    type SnowlumaState,
} from '../../core/domain/events/snowluma-aggregator';

let state: SnowlumaState = initialSnowlumaState;
const listeners = new Set<() => void>();

let subscribePromise: Promise<() => void> | null = null;

function emit(): void {
    for (const fn of listeners) fn();
}

function ensureSubscribed(): void {
    if (subscribePromise) return;
    subscribePromise = eventStreamService.subscribe((event) => {
        const next = reduceSnowluma(state, event);
        if (next === state) return;
        state = next;
        emit();
    });
}

export const snowlumaStore = {
    getSnapshot(): SnowlumaState {
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
        state = initialSnowlumaState;
        emit();
    },
};
