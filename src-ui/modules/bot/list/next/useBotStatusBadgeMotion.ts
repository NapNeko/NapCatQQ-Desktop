import { useEffect, useRef } from 'react';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import type { BotActorSnapshot } from '../../../../core/ipc/types';

export function useBotStatusBadgeMotion(state: BotActorSnapshot['state']) {
    // 状态切换反馈:
    //   - 关键状态转移(starting→running 等) → 状态徽章 pop,而不是整张卡 pop
    //     (大卡 pop 会跟 hover lift / shadow 叠加放大成"整张卡突然鼓一下")
    const { enabled: motionEnabled, pop: popBadge, preset: motionPreset } = useMotion();
    const popPeak = motionPreset.feel.popPeak;
    const badgeRef = useRef<HTMLSpanElement>(null);
    const prevStateRef = useRef<typeof state>(state);

    useEffect(() => {
        const el = badgeRef.current;
        if (!el || !motionEnabled) {
            prevStateRef.current = state;
            return;
        }
        const prev = prevStateRef.current;
        if (prev !== state) {
            const isImpactful =
                (prev === 'starting' && state === 'running') ||
                (prev === 'stopping' && state === 'stopped') ||
                (prev === 'running' && state === 'stopped');
            if (isImpactful && popPeak > 1) {
                popBadge(el);
            }
        }
        prevStateRef.current = state;
    }, [state, motionEnabled, popBadge, popPeak]);

    return badgeRef;
}
