// 任务队列列表选中逻辑：打开时优先选中进行中任务。

import { useEffect, useState } from 'react';
import type { TaskQueueItem } from '../../core/domain/task-queue/types';

function isActiveItem(item: TaskQueueItem): boolean {
    return (
        item.status === 'running' ||
        item.status === 'installing' ||
        item.status === 'pending' ||
        item.status === 'paused'
    );
}

export function pickDefaultTaskId(items: TaskQueueItem[]): string | null {
    const firstActive = items.find(isActiveItem);
    return firstActive?.id ?? items[0]?.id ?? null;
}

export function useTaskQueueSelection(items: TaskQueueItem[]): {
    selectedId: string | null;
    setSelectedId: (id: string | null) => void;
    selected: TaskQueueItem | null;
} {
    const [selectedId, setSelectedId] = useState<string | null>(null);

    useEffect(() => {
        if (selectedId && items.some((i) => i.id === selectedId)) return;
        setSelectedId(pickDefaultTaskId(items));
    }, [items, selectedId]);

    const selected = items.find((i) => i.id === selectedId) ?? null;
    return { selectedId, setSelectedId, selected };
}