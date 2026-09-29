// 各协议端下有几个 Bot。设置 · 功能判断能不能关某个协议端用。

import { useMemo } from 'react';
import { useBotSnapshots } from './useBotSnapshots';
import { useBotFlavorMap } from './useBotFlavorMap';

export function useBotBackendCounts(): { napcat: number; snowluma: number } {
    const { data: snapshots = [] } = useBotSnapshots({ disablePolling: true });
    const flavors = useBotFlavorMap(snapshots);
    return useMemo(() => {
        const counts = { napcat: 0, snowluma: 0 };
        for (const s of snapshots) {
            const flavor = flavors[s.bot_id];
            if (flavor) counts[flavor] += 1;
        }
        return counts;
    }, [snapshots, flavors]);
}
