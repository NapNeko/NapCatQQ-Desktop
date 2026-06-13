// 全应用共享一份 Tauri 事件订阅（22 路 listen 只建一次），按 handler 分发。
// 避免每个 useDomainEvents 实例各订一套 listen（WebView2 订阅乘数 + IPC 解析开销）。

import type { DomainEvent } from '../ipc/types';
import {
    eventStreamService,
    type DomainEventCallback,
    type UnsubscribeFn,
} from './event-stream.service';

type Handler = DomainEventCallback;

const handlers = new Set<Handler>();
let teardown: UnsubscribeFn | null = null;
let startPromise: Promise<void> | null = null;

function dispatch(event: DomainEvent): void {
    for (const handler of handlers) {
        handler(event);
    }
}

function ensureStarted(): Promise<void> {
    if (!startPromise) {
        startPromise = eventStreamService.subscribe(dispatch).then((unsub) => {
            if (handlers.size === 0) {
                unsub();
                teardown = null;
                startPromise = null;
                return;
            }
            teardown = unsub;
        });
    }
    return startPromise;
}

/**
 * 订阅合并 DomainEvent 流。返回的函数与 useEffect cleanup 对齐调用即可。
 * 最后一个 handler 移除时会 unlisten 全部 22 路通道。
 */
export function subscribeDomainEvents(handler: Handler): () => void {
    handlers.add(handler);
    void ensureStarted().catch((err) => {
        // eslint-disable-next-line no-console
        console.error('[domain-event-hub] failed to start event stream:', err);
    });

    return () => {
        handlers.delete(handler);
        if (handlers.size === 0 && teardown) {
            try {
                teardown();
            } catch {
                /* noop */
            }
            teardown = null;
            startPromise = null;
        }
    };
}

/** 测试 / HMR 复位：清空 handler 并拆掉底层 listen。 */
export function _resetDomainEventHubForTests(): void {
    handlers.clear();
    if (teardown) {
        try {
            teardown();
        } catch {
            /* noop */
        }
    }
    teardown = null;
    startPromise = null;
}