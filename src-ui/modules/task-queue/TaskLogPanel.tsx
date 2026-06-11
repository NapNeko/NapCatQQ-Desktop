// 任务详情内嵌 Desktop 日志片段（轮询 tail + startedAt 过滤）。

import React, { useMemo } from 'react';
import { useDesktopLogStream, DESKTOP_LOG_POLL_MS_FAST } from '../../hooks/diagnostics/useDesktopLogStream';
import { filterDesktopLogLinesSince } from '../../core/domain/task-queue/filterDesktopLogs';
import { parseDesktopLogLine } from '../../core/domain/events/log-buffer';
import { Spinner } from '../../shared/ui';

const MAX_LINES = 120;

export interface TaskLogPanelProps {
    startedAt: number;
    enabled: boolean;
    /** 任务队列详情打开时用更短轮询间隔 */
    fastPoll?: boolean;
}

export const TaskLogPanel: React.FC<TaskLogPanelProps> = ({ startedAt, enabled, fastPoll }) => {
    const { logs, loading, error } = useDesktopLogStream('ALL_', enabled, {
        pollIntervalMs: fastPoll ? DESKTOP_LOG_POLL_MS_FAST : undefined,
    });

    const lines = useMemo(() => {
        const raw = logs.map((e) => e.rawLine ?? e.text);
        const filtered = filterDesktopLogLinesSince(raw, startedAt);
        return filtered.slice(-MAX_LINES);
    }, [logs, startedAt]);

    if (!enabled) return null;

    if (loading && lines.length === 0) {
        return (
            <div className="flex items-center gap-2 py-6 text-[12px] text-text-secondary">
                <Spinner size="sm" />
                加载日志…
            </div>
        );
    }

    if (error) {
        return <p className="py-2 text-[12px] text-danger">{error}</p>;
    }

    if (lines.length === 0) {
        return (
            <p className="py-2 text-[12px] text-text-secondary">
                暂无匹配日志。安装类任务阶段说明见上方；完整日志可在设置中查看。
            </p>
        );
    }

    return (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border-subtle bg-inset/40">
            <div className="min-h-0 flex-1 overflow-y-auto p-3 font-sans text-[12px] leading-[1.55] antialiased">
                {lines.map((line, i) => {
                    const p = parseDesktopLogLine(line);
                    return (
                        <div
                            key={`${i}-${p.raw.slice(0, 24)}`}
                            className="grid min-w-0 grid-cols-[4.25rem_minmax(0,1fr)] gap-x-2 gap-y-0.5 py-0.5 text-text-secondary"
                        >
                            {p.timestamp ? (
                                <span className="tabular-nums text-[11px] text-text-tertiary">
                                    {p.timestamp}
                                </span>
                            ) : (
                                <span />
                            )}
                            <span className="min-w-0 break-words text-text">{p.message || p.raw}</span>
                        </div>
                    );
                })}
            </div>
        </div>
    );
};