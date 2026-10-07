// Bot 日志流薄视图：缓冲与订阅在 botLogStore（模块级，永不卸载），
// 这里只做 React 适配 + 开页 hydrate。别再往本文件加 useDomainEvents——
// 订阅随组件卸载断掉就是丢行的来源，见 botLogStore.ts 头注。

import { useCallback, useEffect, useMemo, useSyncExternalStore } from 'react';
import { botLogStore, clearBotLogs, hydrateBotLogs } from './botLogStore';
import type { LogEntry } from '../../core/domain/events/log-buffer';

const EMPTY: LogEntry[] = [];

export function useBotLogStream(botId: string) {
    useEffect(() => {
        hydrateBotLogs(botId);
    }, [botId]);

    const snapshot = useSyncExternalStore(
        botLogStore.subscribe,
        botLogStore.getSnapshot,
        botLogStore.getSnapshot,
    );
    const logs = useMemo(() => snapshot.byId[botId] ?? EMPTY, [botId, snapshot]);
    const clear = useCallback(() => clearBotLogs(botId), [botId]);

    return { logs, clear };
}
