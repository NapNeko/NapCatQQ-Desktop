// 相对时间展示用：在 active 为 true 时按间隔推进 now，避免列表耗时冻结。

import { useEffect, useState } from 'react';

export function useNowMs(active: boolean, intervalMs = 1000): number {
    const [now, setNow] = useState(() => Date.now());

    useEffect(() => {
        if (!active) return;
        setNow(Date.now());
        const id = window.setInterval(() => setNow(Date.now()), intervalMs);
        return () => window.clearInterval(id);
    }, [active, intervalMs]);

    return now;
}