// 全局 DomainEvent 订阅 hook。
// 把 `eventStreamService.subscribe` 包装成稳定的 React effect，handler 用 ref
// 锁定避免重订阅风暴。

import { useEffect, useRef } from 'react';
import { eventStreamService } from '../../core/services/event-stream.service';
import type { DomainEvent } from '../../core/ipc/types';

export type DomainEventHandler = (event: DomainEvent) => void;

export function useDomainEvents(handler: DomainEventHandler): void {
    const handlerRef = useRef(handler);
    handlerRef.current = handler;

    useEffect(() => {
        let unsubscribe: (() => void) | undefined;
        let cancelled = false;

        eventStreamService
            .subscribe((event) => {
                if (cancelled) return;
                handlerRef.current(event);
            })
            .then((unsub) => {
                if (cancelled) {
                    unsub();
                } else {
                    unsubscribe = unsub;
                }
            });

        return () => {
            cancelled = true;
            if (unsubscribe) unsubscribe();
        };
    }, []);
}
