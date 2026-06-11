// 设置页 Desktop 会话日志：Tab 可见时轮询 tail（避免 desktop_log 事件风暴卡死 UI）。

import { useCallback, useEffect, useRef, useState } from 'react';
import {
    buildDesktopHistoryEntries,
    type LogEntry,
} from '../../core/domain/events/log-buffer';
import {
    desktopLevelToIpcFilter,
    type DesktopLogLevelFilterValue,
} from '../../core/domain/desktop-log';
import { desktopLogService } from '../../core/services/desktop.service';

export const DESKTOP_LOG_POLL_MS = 1500;
export const DESKTOP_LOG_POLL_MS_FAST = 500;
const TAIL_LINES = 800;

export function useDesktopLogStream(
    levelFilter: DesktopLogLevelFilterValue,
    enabled: boolean,
    options?: { pollIntervalMs?: number },
) {
    const [logs, setLogs] = useState<LogEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const firstLoad = useRef(true);

    const reload = useCallback(
        async (opts?: { showSpinner?: boolean }) => {
            if (!enabled) return;
            const showSpinner = opts?.showSpinner ?? false;
            if (showSpinner) setLoading(true);
            try {
                const snap = await desktopLogService.tailLog(
                    TAIL_LINES,
                    desktopLevelToIpcFilter(levelFilter),
                );
                setLogs(buildDesktopHistoryEntries(snap.lines));
                setError(null);
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                setError(msg);
                setLogs([]);
            } finally {
                if (showSpinner) setLoading(false);
            }
        },
        [levelFilter, enabled],
    );

    const pollMs = options?.pollIntervalMs ?? DESKTOP_LOG_POLL_MS;

    useEffect(() => {
        if (!enabled) {
            setLoading(false);
            firstLoad.current = true;
            return;
        }
        void reload({ showSpinner: firstLoad.current });
        firstLoad.current = false;

        const id = window.setInterval(() => {
            void reload({ showSpinner: false });
        }, pollMs);
        return () => window.clearInterval(id);
    }, [enabled, reload, pollMs]);

    const manualReload = useCallback(() => reload({ showSpinner: true }), [reload]);

    return { logs, loading, error, reload: manualReload };
}