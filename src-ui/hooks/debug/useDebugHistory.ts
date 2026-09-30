// 调用历史：列表（先显示上一页 / 缓存，后台刷新）、单条完整记录、清空。

import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugHistoryEntry } from '../../core/ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistoryPage } from '../../core/ipc/generated/debug/DebugHistoryPage';
import type { DebugHistoryQuery } from '../../core/ipc/generated/debug/DebugHistoryQuery';
import {
    debugHistoryEntryKey,
    debugHistoryEntryPrefix,
    debugHistoryKey,
    debugHistoryPrefix,
    debugIdleKey,
} from './keys';

export function useDebugHistory(query: DebugHistoryQuery) {
    return useQuery<DebugHistoryPage, Error>({
        queryKey: debugHistoryKey(query),
        queryFn: async () => {
            try {
                return await onebotDebugService.history(query);
            } catch (err) {
                pushErrorBar({ key: 'debug-history', title: '读取调用历史失败', raw: errorText(err) });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        // 改筛选条件 / 翻页的途中继续显示上一页，列表不闪空
        placeholderData: keepPreviousData,
    });
}

/** 一条历史记录写入后不会再变，缓存到用完；记录不存在（被清空了）时数据是 null */
export function useHistoryEntry(id: string | null) {
    return useQuery<DebugHistoryEntry | null, Error>({
        queryKey: id ? debugHistoryEntryKey(id) : debugIdleKey,
        queryFn: async () => {
            if (!id) return null;
            try {
                return await onebotDebugService.historyEntry(id);
            } catch (err) {
                pushErrorBar({ key: `debug-history-entry:${id}`, title: '读取历史记录失败', raw: errorText(err) });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled: !!id,
        staleTime: Infinity,
    });
}

export function useClearHistory() {
    const client = useQueryClient();
    return useMutation<void, unknown, void>({
        mutationFn: () => onebotDebugService.clearHistory(),
        onSuccess: () => {
            void client.invalidateQueries({ queryKey: debugHistoryPrefix });
            // 单条缓存里的记录已经不存在了，别再让详情面板显示它
            client.removeQueries({ queryKey: debugHistoryEntryPrefix });
        },
        onError: (err) => {
            pushErrorBar({ key: 'debug-history-clear', title: '清空调用历史失败', raw: errorText(err) });
        },
    });
}
