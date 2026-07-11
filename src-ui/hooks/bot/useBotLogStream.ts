// Bot 日志流：开页 tail 历史 + 增量订阅。
//
// 缓冲挂在模块级 Map（按 botId），页面 unmount 不清空。
// 否则 SL 离开日志页再回来只能靠 tailLog；远端 bot_*.log 滤完常为空，
// 而运行中看到的多是 snowluma_daemon_log 增量，磁盘历史对不上就会「没日志」。

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

const cacheByBot = new Map<string, LogEntry[]>();

function readCache(botId: string): LogEntry[] {
    return cacheByBot.get(botId) ?? [];
}

function writeCache(botId: string, logs: LogEntry[]) {
    cacheByBot.set(botId, logs);
}

export function useBotLogStream(botId: string) {
    const [logs, setLogs] = useState<LogEntry[]>(() => readCache(botId));
    const backendRef = useRef<'napcat' | 'snowluma' | null>(null);
    const botIdRef = useRef(botId);
    botIdRef.current = botId;

    useEffect(() => {
        setLogs(readCache(botId));
        backendRef.current = null;
        botService
            .getConfig(botId)
            .then((cfg) => {
                if (!cfg) return;
                backendRef.current = cfg.bot.backend_type;
            })
            .catch(() => {});
    }, [botId]);

    // 历史快照：有磁盘尾部则覆盖缓存；空结果保留内存缓存（SL 常见）
    useEffect(() => {
        let cancelled = false;
        botService
            .tailLog(botId, 1000)
            .then((snapshot) => {
                if (cancelled) return;
                const historical = buildHistoryEntries(snapshot.lines);
                if (historical.length === 0) {
                    // 不 setLogs([])：避免把会话内已累积的 daemon 增量清掉
                    return;
                }
                writeCache(botId, historical);
                setLogs(historical);
            })
            .catch((err) => {
                // eslint-disable-next-line no-console
                console.warn('加载 Bot 历史日志失败:', err);
            });

        return () => {
            cancelled = true;
        };
    }, [botId]);

    useDomainEvents((event) => {
        const id = botIdRef.current;
        if (event.kind === 'bot_log_appended' && event.bot_id === id) {
            if (
                backendRef.current === 'snowluma' &&
                event.line.includes('[NapCat]')
            ) {
                return;
            }
            const channel = normalizeChannel(event.channel);
            setLogs((prev) => {
                const next = appendLine(prev, event.line, channel);
                writeCache(id, next);
                return next;
            });
            return;
        }

        if (event.kind === 'snowluma_daemon_log') {
            if (backendRef.current !== 'snowluma') {
                return;
            }
            setLogs((prev) => {
                const next = appendLine(
                    prev,
                    event.line,
                    snowlumaLineChannel(event.line),
                );
                writeCache(id, next);
                return next;
            });
            return;
        }

        if (event.kind === 'bot_process_exited' && event.bot_id === id) {
            writeCache(id, []);
            setLogs([]);
        }
    });

    const clear = () => {
        writeCache(botId, []);
        setLogs([]);
    };

    return { logs, clear };
}
