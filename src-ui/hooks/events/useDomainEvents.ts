// 全局 DomainEvent 订阅 hook。
// 经 domain-event-hub 订阅；handler 用 ref 锁定，避免 effect 重跑时重订阅。

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
