// SnowLuma daemon + per-bot 聚合 store（模块级单例）。
// state 持久不丢，事件订阅一次（首个 React 订阅者来时挂上），路由切回不重置。

import { createStore } from '../utils/createStore';
import { eventStreamService } from '../../core/services/event-stream.service';
import {
    initialSnowlumaState,
    reduceSnowluma,
    type SnowlumaState,
} from '../../core/domain/events/snowluma-aggregator';

const store = createStore<SnowlumaState>(initialSnowlumaState);

let subscribePromise: Promise<() => void> | null = null;

function ensureSubscribed(): void {
    if (subscribePromise) return;
    subscribePromise = eventStreamService.subscribe((event) => {
        const next = reduceSnowluma(store.getSnapshot(), event);
        store.setState(next);
    });
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
    },
};
