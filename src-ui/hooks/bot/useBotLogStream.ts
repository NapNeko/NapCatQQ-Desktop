// Bot 日志流 hook：开页拉一次历史快照 + 增量订阅。

import { useEffect, useState } from 'react';
import { botService } from '../../core/services/bot.service';
import { useDomainEvents } from '../events/useDomainEvents';
import {
    appendLine,
    buildHistoryEntries,
    normalizeChannel,
    snowlumaLineChannel,
    type LogEntry,
} from '../../core/domain/events/log-buffer';

export function useBotLogStream(botId: string) {
    const [logs, setLogs] = useState<LogEntry[]>([]);

    // 1) 历史快照（开页一次）。
    useEffect(() => {
        let cancelled = false;
        botService
            .tailLog(botId, 1000)
            .then((snapshot) => {
                if (cancelled) return;
                const historical = buildHistoryEntries(snapshot.lines);
                if (historical.length > 0) setLogs(historical);
            })
            .catch((err) => {
                // eslint-disable-next-line no-console
                console.warn('加载 Bot 历史日志失败:', err);
            });

        return () => {
            cancelled = true;
        };
    }, [botId]);

    // 2) 增量事件。
    useDomainEvents((event) => {
        if (event.kind === 'bot_log_appended' && event.bot_id === botId) {
            const channel = normalizeChannel(event.channel);
            setLogs((prev) => appendLine(prev, event.line, channel));
            return;
        }

        // SnowLuma daemon 是单例，每个 SL bot 的 LogPage 都接收同一份 stdout 行。
        // 不按 bot_id 过滤，按 `[stderr]` 前缀分流到对应 channel。
        if (event.kind === 'snowluma_daemon_log') {
            setLogs((prev) => appendLine(prev, event.line, snowlumaLineChannel(event.line)));
            return;
        }

        // bot 进程退出时清空前端日志。两个场景必须清：
        //   1) 用户手动停止 → 紧接着切换 backend (NC ↔ SL) 再启动，旧后端的
        //      行不应该跟新后端混排
        //   2) bot crash 后用户原地重启
        // 后端 spawn_exit_watcher 也会清内存 buffer，但前端 state 是独立的副本，
        // 必须在事件流里同步清掉。tail_log 的历史快照已经在 useEffect 重新拉。
        if (event.kind === 'bot_process_exited' && event.bot_id === botId) {
            setLogs([]);
        }
    });

    const clear = () => setLogs([]);

    return { logs, clear };
}
