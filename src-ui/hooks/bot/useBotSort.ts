// Bot 拖拽排序 Store 与 Hook。
// 负责维护用户自定义的 Bot 卡片排列顺序，并在 LocalStorage 持久化，
// 保证 Bot 列表页与托盘面板共享统一的手动排序。

import { useSyncExternalStore, useMemo, useCallback } from 'react';
import type { BotActorSnapshot } from '../../core/ipc/types';

const STORAGE_KEY = 'ncd:bot_custom_order:v1';
const CHANNEL_NAME = 'ncd:bot_sort_channel:v1';

function loadCustomOrderFromStorage(): string[] {
    try {
        const val = localStorage.getItem(STORAGE_KEY);
        if (val) {
            const parsed = JSON.parse(val);
            if (Array.isArray(parsed)) {
                return parsed.filter((id) => typeof id === 'string' && id.length > 0);
            }
        }
    } catch {
        // ignore
    }
    return [];
}

let customOrder: string[] = loadCustomOrderFromStorage();
const listeners = new Set<() => void>();

const broadcastChannel =
    typeof BroadcastChannel !== 'undefined'
        ? new BroadcastChannel(CHANNEL_NAME)
        : null;

if (broadcastChannel) {
    broadcastChannel.onmessage = (event) => {
        if (event.data?.type === 'ORDER_CHANGED' && Array.isArray(event.data?.order)) {
            customOrder = event.data.order;
            listeners.forEach((l) => l());
        }
    };
}

if (typeof window !== 'undefined') {
    window.addEventListener('storage', (e) => {
        if (e.key === STORAGE_KEY) {
            customOrder = loadCustomOrderFromStorage();
            listeners.forEach((l) => l());
        }
    });
}

export const botSortStore = {
    get: () => customOrder,
    refresh: () => {
        const fresh = loadCustomOrderFromStorage();
        if (JSON.stringify(fresh) !== JSON.stringify(customOrder)) {
            customOrder = fresh;
            listeners.forEach((l) => l());
        }
    },
    set: (newOrder: string[]) => {
        customOrder = newOrder;
        try {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(newOrder));
        } catch {
            // ignore
        }
        listeners.forEach((l) => l());
        broadcastChannel?.postMessage({ type: 'ORDER_CHANGED', order: newOrder });
    },
    reorder: (sourceBotId: string, targetBotId: string, currentBots: BotActorSnapshot[]) => {
        if (sourceBotId === targetBotId) return;
        const currentIds = currentBots.map((b) => b.bot_id);
        const sourceIndex = currentIds.indexOf(sourceBotId);
        const targetIndex = currentIds.indexOf(targetBotId);
        if (sourceIndex === -1 || targetIndex === -1) return;

        const newIds = [...currentIds];
        const temp = newIds[sourceIndex];
        newIds[sourceIndex] = newIds[targetIndex];
        newIds[targetIndex] = temp;

        botSortStore.set(newIds);
    },
    subscribe: (listener: () => void) => {
        listeners.add(listener);
        return () => {
            listeners.delete(listener);
        };
    },
};

export function sortBotSnapshotsByCustomOrder(
    bots: BotActorSnapshot[],
    order: string[],
): BotActorSnapshot[] {
    if (!order || order.length === 0) return bots;
    const orderMap = new Map<string, number>();
    order.forEach((id, idx) => orderMap.set(id, idx));

    return [...bots].sort((a, b) => {
        const idxA = orderMap.has(a.bot_id) ? orderMap.get(a.bot_id)! : 999999;
        const idxB = orderMap.has(b.bot_id) ? orderMap.get(b.bot_id)! : 999999;
        if (idxA !== idxB) return idxA - idxB;
        return a.bot_id.localeCompare(b.bot_id, 'zh-CN', { numeric: true });
    });
}

export function useSortedBots(bots: BotActorSnapshot[]): {
    sortedBots: BotActorSnapshot[];
    reorderBots: (sourceBotId: string, targetBotId: string) => void;
} {
    const order = useSyncExternalStore(
        botSortStore.subscribe,
        botSortStore.get,
    );

    const sortedBots = useMemo(
        () => sortBotSnapshotsByCustomOrder(bots, order),
        [bots, order],
    );

    const reorderBots = useCallback(
        (sourceBotId: string, targetBotId: string) => {
            botSortStore.reorder(sourceBotId, targetBotId, sortedBots);
        },
        [sortedBots],
    );

    return { sortedBots, reorderBots };
}

// 兼容热更新缓存
export const BOT_SORT_OPTIONS = [];
export type BotSortOption = string;
export function useBotSort() {
    return { sortOption: 'custom', setSortOption: () => { } };
}
