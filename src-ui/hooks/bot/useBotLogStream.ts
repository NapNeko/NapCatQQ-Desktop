// Bot 日志流 hook：开页拉一次历史快照 + 增量订阅。

import { useEffect, useRef, useState } from 'react';
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
    const backendRef = useRef<'napcat' | 'snowluma' | null>(null);

    useEffect(() => {
        setLogs([]);
        backendRef.current = null;
        botService
            .getConfig(botId)
            .then((cfg) => {
                if (!cfg) return;
                backendRef.current = cfg.bot.backend_type;
            })
            .catch(() => {});
    }, [botId]);

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
            if (
                backendRef.current === 'snowluma' &&
                event.line.includes('[NapCat]')
            ) {
                return;
            }
            const channel = normalizeChannel(event.channel);
            setLogs((prev) => appendLine(prev, event.line, channel));
            return;
        }

        if (event.kind === 'snowluma_daemon_log') {
            if (backendRef.current !== 'snowluma') {
                return;
            }
            setLogs((prev) => appendLine(prev, event.line, snowlumaLineChannel(event.line)));
            return;
        }

        if (event.kind === 'bot_process_exited' && event.bot_id === botId) {
            setLogs([]);
        }
    });

    const clear = () => setLogs([]);

    return { logs, clear };
}