// EventPanel 专用：拿到带 ID + 中文文案的 event 流，最多保留 N 条。

import { useState } from 'react';
import { useDomainEvents } from '../events/useDomainEvents';
import { describeEvent } from '../../core/domain/events/event-label';
import type { DomainEvent } from '../../core/ipc/types';

export interface UiEventRecord {
    id: string;
    timestamp: string;
    kind: string;
    message: string;
    payload: DomainEvent;
}

const MAX_RECORDS = 100;
let counter = 0;
function nextId(): string {
    counter += 1;
    return `evt-${Date.now()}-${counter}`;
}

export function useEventStream(maxRecords = MAX_RECORDS) {
    const [events, setEvents] = useState<UiEventRecord[]>([]);

    useDomainEvents((event) => {
        const desc = describeEvent(event);
        const record: UiEventRecord = {
            id: nextId(),
            timestamp: new Date().toLocaleTimeString(),
            kind: event.kind,
            message: desc.message,
            payload: event,
        };
        setEvents((prev) => [record, ...prev].slice(0, maxRecords));
    });

    return {
        events,
        clear: () => setEvents([]),
    };
}
