// 设置页 Desktop 会话日志：Tab 可见时轮询 tail（避免 desktop_log 事件风暴卡死 UI）。
// 轮询交给 useQuery 的 refetchInterval；lines 没变时 queryFn 原样返回上一份 entries
// （buildDesktopHistoryEntries 每行新生成 id，react-query 的结构共享判不出来），
// 等价于旧实现手写的 lastLines 比对，省掉整屏日志的无谓重渲。

import { useCallback, useEffect, useRef, useState } from 'react';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import { buildDesktopHistoryEntries, type LogEntry } from '../../core/domain/events/log-buffer';
import {
    desktopLevelToIpcFilter,
    type DesktopLogLevelFilterValue,
} from '../../core/domain/desktop-log';
import { desktopLogService } from '../../core/services/desktop.service';

export const DESKTOP_LOG_POLL_MS = 1500;
export const DESKTOP_LOG_POLL_MS_FAST = 500;
const TAIL_LINES = 800;

const EMPTY_LOGS: LogEntry[] = [];

export function useDesktopLogStream(
    levelFilter: DesktopLogLevelFilterValue,
    enabled: boolean,
    options?: { pollIntervalMs?: number },
) {
    const client = useQueryClient();
    const pollMs = options?.pollIntervalMs ?? DESKTOP_LOG_POLL_MS;
    const lastRef = useRef<{ lines: string[]; entries: LogEntry[] } | null>(null);

    useEffect(() => {
        if (enabled) return;
        // 关 Tab 就作废缓存：重新开启要重新拉并回到 loading（对齐旧实现的 firstLoad 语义）
        void client.removeQueries({ queryKey: ['desktopLog', levelFilter] });
    }, [enabled, levelFilter, client]);

    const query = useQuery({
        queryKey: ['desktopLog', levelFilter],
        queryFn: async () => {
            try {
                const snap = await desktopLogService.tailLog(
                    TAIL_LINES,
                    desktopLevelToIpcFilter(levelFilter),
                );
                const lines = snap.lines;
                const prev = lastRef.current;
                if (
                    prev &&
                    lines.length === prev.lines.length &&
                    lines.every((l, i) => l === prev.lines[i])
                ) {
                    return prev.entries;
                }
                const entries = buildDesktopHistoryEntries(lines);
                lastRef.current = { lines, entries };
                return entries;
            } catch (err) {
                // 失败后下一轮哪怕内容相同也重建（旧实现出错清 lastLines 的语义）
                lastRef.current = null;
                throw err;
            }
        },
        enabled,
        refetchInterval: pollMs,
        // 旧实现是裸 setInterval：窗口失焦照拉，不能让 react-query 省掉后台轮次
        refetchIntervalInBackground: true,
        // tail 要的是当下的内容，不吃缓存
        staleTime: 0,
        // 离开设置页（卸载）后缓存即弃：否则 refetchInterval 会在后台继续轮 tail 五分钟
        gcTime: 0,
        // 切等级筛选时上一份日志先留在屏幕上，不闪 loading（旧实现即如此）
        placeholderData: keepPreviousData,
    });

    // 手动刷新要像旧实现一样亮 spinner，轮询不亮，所以单独记一个标记
    const [manualFetching, setManualFetching] = useState(false);
    const { refetch } = query;
    const reload = useCallback(async () => {
        if (!enabled) return;
        setManualFetching(true);
        try {
            await refetch();
        } finally {
            setManualFetching(false);
        }
    }, [enabled, refetch]);

    const logs = query.isError ? EMPTY_LOGS : (query.data ?? EMPTY_LOGS);
    // 切筛选保留旧数据时 isPending 仍为真，用「没有可展示数据」判首次加载，避免闪 spinner
    const loading = enabled && ((query.isPending && query.data === undefined) || manualFetching);
    const error = query.isError
        ? query.error instanceof Error
            ? query.error.message
            : String(query.error)
        : null;

    return { logs, loading, error, reload };
}
