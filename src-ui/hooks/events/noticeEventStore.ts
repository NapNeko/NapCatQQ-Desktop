// 概览页「最近通知」的运行时事件缓冲（模块级单例）。
//
// 之前用组件内 useState 存全量事件流，两个坑：
//   1. 离开概览再回来组件重挂，崩溃 / 掉线通知全部消失；
//   2. bot_log_appended 之类高频事件也进缓冲，100 条上限几秒就被日志行冲掉，
//      而且每条日志都让整个概览页重渲染。
// 现在只收 isNoticeEvent 认可的事件，带收到时刻，跨路由保留。
// 订阅在首个 React 订阅者来时挂一次，之后不卸，保证切到别的页面也在攒。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import {
    isNoticeEvent,
    type NoticeEventRecord,
} from '../../core/domain/events/notice-aggregator';
import type { DomainEvent } from '../../core/ipc/types';

export interface StoredNoticeEvent extends NoticeEventRecord {
    id: string;
    at: number;
}

interface State {
    /** 最新在前。 */
    events: StoredNoticeEvent[];
}

const MAX_RECORDS = 50;

const store = createStore<State>({ events: [] });

let counter = 0;
let unsubDomain: (() => void) | null = null;

function onDomainEvent(event: DomainEvent): void {
    if (!isNoticeEvent(event)) return;
    counter += 1;
    const record: StoredNoticeEvent = {
        id: `notice-evt-${counter}`,
        at: Date.now(),
        payload: event,
    };
    const current = store.getSnapshot();
    store.setState({
        events: [record, ...current.events].slice(0, MAX_RECORDS),
    });
}

function ensureSubscribed(): void {
    if (unsubDomain) return;
    unsubDomain = subscribeDomainEvents(onDomainEvent);
}

export const noticeEventStore = {
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
        counter = 0;
        store._reset();
    },
};

export function useNoticeEvents(): StoredNoticeEvent[] {
    return useSyncExternalStore(
        noticeEventStore.subscribe,
        noticeEventStore.getSnapshot,
        noticeEventStore.getSnapshot,
    ).events;
}
