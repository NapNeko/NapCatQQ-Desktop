// 全局 DomainEvent 订阅 hook。
// 把 `eventStreamService.subscribe` 包装成稳定的 React effect，handler 用 ref
// 锁定避免重订阅风暴。

import { useEffect, useRef } from 'react';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import type { DomainEvent } from '../../core/ipc/types';

export type DomainEventHandler = (event: DomainEvent) => void;

export function useDomainEvents(handler: DomainEventHandler): void {
    const handlerRef = useRef(handler);
    handlerRef.current = handler;

    useEffect(() => {
        return subscribeDomainEvents((event) => {
            handlerRef.current(event);
        });
    }, []);
}
