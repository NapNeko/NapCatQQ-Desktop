// 组件页 QQ 依赖探测：按主机缓存探测结果 + 在途去重 + 选中主机自动探测。
// 探测结果不进 react-query（每台远端探测要走 SSH，且失败文案要按主机单独展示），
// 从 ComponentsPage.next 外提，页面只拿 probe 函数和 reportByHost。

import { useCallback, useEffect, useRef, useState } from 'react';
import {
    canProbeQqDependencies,
    type QqDependencyProbeState,
} from '../../core/domain/components/qqProbe';
import { errorText } from '../../core/domain/errors';
import type { MachineView } from '../../core/domain/components/types';
import type { QqDependencyReport } from '../../core/ipc/generated/qq/QqDependencyReport';

export function useQqDependencyProbes(
    machines: MachineView[],
    activeMachine: MachineView | null,
    detectQqDependencies: (hostId: string) => Promise<QqDependencyReport>,
): {
    probeQqDependencies: (hostId: string, force?: boolean) => Promise<void>;
    reportByHost: Record<string, QqDependencyProbeState | undefined>;
} {
    const [qqDependencyByHost, setQqDependencyByHost] = useState<
        Record<string, QqDependencyProbeState | undefined>
    >({});
    const qqDependencyInFlightRef = useRef<Set<string>>(new Set());

    const probeQqDependencies = useCallback(
        async (hostId: string, force = false) => {
            const machine = machines.find((m) => m.host.host_id === hostId);
            if (!canProbeQqDependencies(machine)) return;

            const current = qqDependencyByHost[hostId];
            if (!force && current) {
                return;
            }
            if (qqDependencyInFlightRef.current.has(hostId)) return;

            qqDependencyInFlightRef.current.add(hostId);
            setQqDependencyByHost((prev) => ({
                ...prev,
                [hostId]: { status: 'loading', report: null, error: null },
            }));
            try {
                const report = await detectQqDependencies(hostId);
                setQqDependencyByHost((prev) => ({
                    ...prev,
                    [hostId]: { status: 'ready', report, error: null },
                }));
            } catch (err) {
                const message = errorText(err, 'QQ 依赖探测失败');
                setQqDependencyByHost((prev) => ({
                    ...prev,
                    [hostId]: { status: 'error', report: null, error: message },
                }));
                console.warn('[ComponentsPage] QQ dependency probe failed:', err);
            } finally {
                qqDependencyInFlightRef.current.delete(hostId);
            }
        },
        [machines, qqDependencyByHost, detectQqDependencies],
    );

    useEffect(() => {
        if (!canProbeQqDependencies(activeMachine)) return;
        void probeQqDependencies(activeMachine.host.host_id);
    }, [activeMachine, probeQqDependencies]);

    return { probeQqDependencies, reportByHost: qqDependencyByHost };
}
