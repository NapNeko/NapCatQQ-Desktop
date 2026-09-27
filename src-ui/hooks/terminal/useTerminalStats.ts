// 服务器状态条：看得见的时候每 3 秒读一次；读失败保留上一次的数，标一下「没读到」。

import { useEffect, useState } from 'react';
import { terminalService } from '../../core/services/terminal.service';
import type { ServerStats } from '../../core/ipc/generated/domain/ServerStats';

const INTERVAL_MS = 3000;

export function useTerminalStats(sessionId: string, enabled: boolean): { stats: ServerStats | null; stale: boolean } {
    const [stats, setStats] = useState<ServerStats | null>(null);
    const [stale, setStale] = useState(false);

    useEffect(() => {
        if (!enabled) return;
        let cancelled = false;
        let timer: number | undefined;
        const tick = async () => {
            try {
                const next = await terminalService.stats(sessionId);
                if (cancelled) return;
                setStats(next);
                setStale(false);
            } catch {
                if (!cancelled) setStale(true);
            } finally {
                if (!cancelled) timer = window.setTimeout(tick, INTERVAL_MS);
            }
        };
        void tick();
        return () => {
            cancelled = true;
            if (timer !== undefined) window.clearTimeout(timer);
        };
    }, [sessionId, enabled]);

    return { stats, stale };
}
