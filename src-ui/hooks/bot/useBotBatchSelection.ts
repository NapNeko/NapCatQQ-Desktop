// 批量选择模式 state 隔离 hook。

import { useCallback, useState } from 'react';

export function useBotBatchSelection() {
    const [isBatchMode, setIsBatchMode] = useState(false);
    const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

    const toggleSelect = useCallback((id: string) => {
        setSelectedIds((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    }, []);

    const enterBatch = useCallback(() => setIsBatchMode(true), []);
    const exitBatch = useCallback(() => {
        setIsBatchMode(false);
        setSelectedIds(new Set());
    }, []);
    const toggleBatch = useCallback(() => {
        setIsBatchMode((prev) => {
            const next = !prev;
            if (!next) setSelectedIds(new Set());
            return next;
        });
    }, []);

    return {
        isBatchMode,
        selectedIds,
        toggleSelect,
        enterBatch,
        exitBatch,
        toggleBatch,
    };
}
