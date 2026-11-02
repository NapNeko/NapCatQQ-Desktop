import { useMemo, useSyncExternalStore } from 'react';
import type { ChatSnapshot } from './chatStore.types';
import type { ChatAccountStore } from './ChatAccountStore';

export function useChatSnapshot(store: ChatAccountStore) {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

export function useChatSelector<T>(
    store: ChatAccountStore,
    selector: (snapshot: ChatSnapshot) => T,
    isEqual: (previous: T, next: T) => boolean = Object.is,
): T {
    const getSelection = useMemo(() => {
        let previousSnapshot: ChatSnapshot | undefined;
        let selection: T;
        return () => {
            const snapshot = store.getSnapshot();
            if (snapshot !== previousSnapshot) {
                const next = selector(snapshot);
                if (!previousSnapshot || !isEqual(selection, next)) selection = next;
                previousSnapshot = snapshot;
            }
            return selection;
        };
    }, [store, selector, isEqual]);
    return useSyncExternalStore(store.subscribe, getSelection, getSelection);
}
