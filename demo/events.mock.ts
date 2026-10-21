import type { DomainEvent } from '@desktop/core/ipc/types';

type EventCallback = (event: DomainEvent) => void;
const callbacks = new Set<EventCallback>();

export function emitMockEvent(event: DomainEvent): void {
  for (const callback of callbacks) callback(event);
}

export function subscribeMockEvents(callback: EventCallback): () => void {
  callbacks.add(callback);
  return () => { callbacks.delete(callback); };
}
